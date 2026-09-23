use std::collections::VecDeque;
use std::ffi::{c_void, OsStr};
use std::fs::File;
use std::io::Read;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, STILL_ACTIVE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use super::require_execution_gate;

const MAX_LOG_BYTES: usize = 64 * 1024;
const FORCED_EXIT_CODE: u32 = 0xA11D_E001;

#[derive(Clone, Debug)]
pub(crate) struct ProcessSpec {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
}

impl ProcessSpec {
    fn validate(&self) -> Result<(), String> {
        if !self.executable.is_absolute() || !self.working_directory.is_absolute() {
            return Err("Managed process paths must be absolute.".into());
        }
        if !self.executable.is_file() || !self.working_directory.is_dir() {
            return Err("Managed process paths do not exist.".into());
        }
        if self
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
        {
            return Err("Managed process arguments contain an invalid null character.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ComfyUiProcessPaths {
    pub runtime_root: PathBuf,
    pub data_root: PathBuf,
    pub models_root: PathBuf,
}

pub(crate) fn managed_comfyui_process_spec(
    paths: &ComfyUiProcessPaths,
    port: u16,
) -> Result<ProcessSpec, String> {
    require_execution_gate()?;
    fixed_comfyui_process_spec(paths, port)
}

fn fixed_comfyui_process_spec(
    paths: &ComfyUiProcessPaths,
    port: u16,
) -> Result<ProcessSpec, String> {
    if port == 0
        || !paths.runtime_root.is_absolute()
        || !paths.data_root.is_absolute()
        || !paths.models_root.is_absolute()
        || paths.data_root.starts_with(&paths.runtime_root)
        || paths.models_root.starts_with(&paths.runtime_root)
        || paths.models_root.starts_with(&paths.data_root)
        || paths.data_root.starts_with(&paths.models_root)
    {
        return Err("Managed ComfyUI paths or session port are invalid.".into());
    }
    let executable = paths.runtime_root.join("python_embeded/python.exe");
    let main = paths.runtime_root.join("ComfyUI/main.py");
    let working_directory = paths.runtime_root.join("ComfyUI");
    let spec = ProcessSpec {
        executable,
        arguments: vec![
            main.to_string_lossy().into_owned(),
            "--listen".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--base-directory".into(),
            paths.data_root.to_string_lossy().into_owned(),
            "--temp-directory".into(),
            paths.data_root.join("temp").to_string_lossy().into_owned(),
            "--user-directory".into(),
            paths.data_root.join("user").to_string_lossy().into_owned(),
            "--models-directory".into(),
            paths.models_root.to_string_lossy().into_owned(),
            "--disable-auto-launch".into(),
            "--disable-all-custom-nodes".into(),
            "--disable-api-nodes".into(),
        ],
        working_directory,
    };
    spec.validate()?;
    Ok(spec)
}

#[derive(Default)]
struct BoundedLog {
    bytes: VecDeque<u8>,
}

impl BoundedLog {
    fn push(&mut self, chunk: &[u8]) {
        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(MAX_LOG_BYTES);
        self.bytes.drain(..overflow.min(self.bytes.len()));
        self.bytes.extend(chunk);
    }

    fn text(&self) -> String {
        let bytes: Vec<u8> = self.bytes.iter().copied().collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn take(&mut self) -> HANDLE {
        let handle = self.0;
        self.0 = null_mut();
        handle
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the wrapper exclusively owns this valid Win32 handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct PipeSet {
    stdin_read: OwnedHandle,
    stdin_write: OwnedHandle,
    stdout_read: OwnedHandle,
    stdout_write: OwnedHandle,
    stderr_read: OwnedHandle,
    stderr_write: OwnedHandle,
}

impl PipeSet {
    fn create() -> Result<Self, String> {
        let (stdin_read, stdin_write) = create_inheritable_pipe(false)?;
        let (stdout_read, stdout_write) = create_inheritable_pipe(true)?;
        let (stderr_read, stderr_write) = create_inheritable_pipe(true)?;
        Ok(Self {
            stdin_read,
            stdin_write,
            stdout_read,
            stdout_write,
            stderr_read,
            stderr_write,
        })
    }
}

fn create_inheritable_pipe(parent_owns_read: bool) -> Result<(OwnedHandle, OwnedHandle), String> {
    let mut read = null_mut();
    let mut write = null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    // SAFETY: pointers refer to initialized output slots and a valid attributes structure.
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(last_error("Could not create a managed-process log pipe"));
    }
    let read = OwnedHandle(read);
    let write = OwnedHandle(write);
    let parent_handle = if parent_owns_read { read.0 } else { write.0 };
    // SAFETY: parent_handle is a live handle created immediately above.
    if unsafe { SetHandleInformation(parent_handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(last_error(
            "Could not restrict managed-process pipe inheritance",
        ));
    }
    Ok((read, write))
}

struct AttributeList {
    storage: Vec<u8>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn handles(handles: &mut [HANDLE]) -> Result<Self, String> {
        let mut bytes = 0_usize;
        // SAFETY: the documented first call queries the required allocation size.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(last_error("Could not size the managed-process handle list"));
        }
        let mut storage = vec![0_u8; bytes];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: storage is retained by Self and has the size requested by Windows.
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(last_error(
                "Could not initialize the managed-process handle list",
            ));
        }
        // SAFETY: pointer is initialized and handles contains only live inheritable pipe handles.
        if unsafe {
            UpdateProcThreadAttribute(
                pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast::<c_void>(),
                size_of_val(handles),
                null_mut(),
                null(),
            )
        } == 0
        {
            let error = last_error("Could not restrict managed-process inherited handles");
            // SAFETY: pointer was initialized successfully above.
            unsafe { DeleteProcThreadAttributeList(pointer) };
            return Err(error);
        }
        Ok(Self { storage, pointer })
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        let _ = self.storage.len();
        // SAFETY: pointer was initialized and remains backed by self.storage.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}

pub(crate) struct ManagedProcess {
    job: OwnedHandle,
    process: OwnedHandle,
    stdout: Arc<Mutex<BoundedLog>>,
    stderr: Arc<Mutex<BoundedLog>>,
    readers: Vec<JoinHandle<()>>,
    terminated: bool,
}

impl ManagedProcess {
    pub(crate) fn launch(spec: &ProcessSpec) -> Result<Self, String> {
        spec.validate()?;
        let mut pipes = PipeSet::create()?;
        let mut inherited = [
            pipes.stdin_read.0,
            pipes.stdout_write.0,
            pipes.stderr_write.0,
        ];
        let attributes = AttributeList::handles(&mut inherited)?;
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = pipes.stdin_read.0;
        startup.StartupInfo.hStdOutput = pipes.stdout_write.0;
        startup.StartupInfo.hStdError = pipes.stderr_write.0;
        startup.lpAttributeList = attributes.pointer;

        let job = OwnedHandle(unsafe { CreateJobObjectW(null(), null()) });
        if job.0.is_null() {
            return Err(last_error(
                "Could not create the managed-process Job Object",
            ));
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: limits points to the documented structure for this information class.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(last_error(
                "Could not configure kill-on-close process ownership",
            ));
        }

        let mut application = wide_null(spec.executable.as_os_str());
        let mut command_line = command_line(&spec.executable, &spec.arguments);
        let working_directory = wide_null(spec.working_directory.as_os_str());
        let mut process_info: PROCESS_INFORMATION = unsafe { zeroed() };
        // SAFETY: all pointers remain valid for the duration of CreateProcessW; the child starts suspended.
        let created = unsafe {
            CreateProcessW(
                application.as_mut_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                null(),
                working_directory.as_ptr(),
                (&startup as *const STARTUPINFOEXW).cast(),
                &mut process_info,
            )
        };
        if created == 0 {
            return Err(last_error("Could not create the suspended managed process"));
        }
        let process = OwnedHandle(process_info.hProcess);
        let thread_handle = OwnedHandle(process_info.hThread);

        // SAFETY: both handles were just created by this call and the process has not run.
        if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
            let error = last_error("Could not assign the suspended process to AIIDE ownership");
            unsafe { TerminateProcess(process.0, FORCED_EXIT_CODE) };
            return Err(error);
        }
        // SAFETY: the primary thread is still suspended and owned by this function.
        if unsafe { ResumeThread(thread_handle.0) } == u32::MAX {
            let error = last_error("Could not resume the owned managed process");
            unsafe { TerminateJobObject(job.0, FORCED_EXIT_CODE) };
            return Err(error);
        }

        drop(attributes);
        drop(thread_handle);
        drop(pipes.stdin_read);
        drop(pipes.stdin_write);
        drop(pipes.stdout_write);
        drop(pipes.stderr_write);
        let stdout_handle = pipes.stdout_read.take();
        let stderr_handle = pipes.stderr_read.take();
        let stdout = Arc::new(Mutex::new(BoundedLog::default()));
        let stderr = Arc::new(Mutex::new(BoundedLog::default()));
        let readers = vec![
            spawn_log_reader(stdout_handle, stdout.clone()),
            spawn_log_reader(stderr_handle, stderr.clone()),
        ];

        Ok(Self {
            job,
            process,
            stdout,
            stderr,
            readers,
            terminated: false,
        })
    }

    pub(crate) fn is_running(&self) -> Result<bool, String> {
        Ok(self.exit_code()?.is_none())
    }

    pub(crate) fn exit_code(&self) -> Result<Option<u32>, String> {
        let mut code = 0_u32;
        // SAFETY: self.process is a live process handle owned by this object.
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(last_error("Could not inspect the managed process"));
        }
        Ok((code != STILL_ACTIVE as u32).then_some(code))
    }

    pub(crate) fn wait_for_exit(&self, timeout: Duration) -> Result<Option<u32>, String> {
        let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
        // SAFETY: self.process is a live waitable process handle.
        match unsafe { WaitForSingleObject(self.process.0, milliseconds) } {
            WAIT_OBJECT_0 => self.exit_code(),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(last_error("Could not wait for the managed process")),
        }
    }

    pub(crate) fn log_tail(&self) -> (String, String) {
        let stdout = self.stdout.lock().map(|log| log.text()).unwrap_or_default();
        let stderr = self.stderr.lock().map(|log| log.text()).unwrap_or_default();
        (stdout, stderr)
    }

    pub(crate) fn terminate_owned_tree(&mut self) -> Result<(), String> {
        if self.terminated || self.exit_code()?.is_some() {
            self.terminated = true;
            return Ok(());
        }
        // SAFETY: the live Job Object is the ownership proof for this process tree.
        if unsafe { TerminateJobObject(self.job.0, FORCED_EXIT_CODE) } == 0 {
            return Err(last_error(
                "Could not terminate the owned managed process tree",
            ));
        }
        self.terminated = true;
        Ok(())
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        if !self.terminated {
            // Closing this kill-on-close job is also the crash/early-drop cleanup boundary.
            let _ = unsafe { TerminateJobObject(self.job.0, FORCED_EXIT_CODE) };
        }
        let process = self.process.take();
        if !process.is_null() {
            unsafe { CloseHandle(process) };
        }
        let job = self.job.take();
        if !job.is_null() {
            unsafe { CloseHandle(job) };
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

fn spawn_log_reader(handle: HANDLE, target: Arc<Mutex<BoundedLog>>) -> JoinHandle<()> {
    let handle_value = handle as usize;
    thread::spawn(move || {
        // SAFETY: ownership of this pipe read handle is transferred exclusively to File.
        let mut file = unsafe { File::from_raw_handle(handle_value as *mut c_void) };
        let mut buffer = [0_u8; 4096];
        while let Ok(read) = file.read(&mut buffer) {
            if read == 0 {
                break;
            }
            if let Ok(mut log) = target.lock() {
                log.push(&buffer[..read]);
            }
        }
    })
}

fn command_line(executable: &Path, arguments: &[String]) -> Vec<u16> {
    let mut command = quote_windows_argument(&executable.to_string_lossy());
    for argument in arguments {
        command.push(' ');
        command.push_str(&quote_windows_argument(argument));
    }
    wide_null(OsStr::new(&command))
}

fn quote_windows_argument(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return value.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut slashes = 0_usize;
    for character in value.chars() {
        if character == '\\' {
            slashes += 1;
        } else if character == '"' {
            quoted.push_str(&"\\".repeat(slashes * 2 + 1));
            quoted.push('"');
            slashes = 0;
        } else {
            quoted.push_str(&"\\".repeat(slashes));
            slashes = 0;
            quoted.push(character);
        }
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('"');
    quoted
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn last_error(context: &str) -> String {
    // SAFETY: GetLastError has no preconditions and is read immediately after failure.
    format!("{context} (Windows error {}).", unsafe { GetLastError() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_quoting_preserves_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(quote_windows_argument("two words\\"), "\"two words\\\\\"");
        assert_eq!(quote_windows_argument("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn comfyui_launch_contract_is_fixed_and_production_entry_is_gated() {
        let root = std::env::temp_dir().join(format!("aiide-comfy-spec-{}", std::process::id()));
        let paths = ComfyUiProcessPaths {
            runtime_root: root.join("runtime"),
            data_root: root.join("data"),
            models_root: root.join("models"),
        };
        std::fs::create_dir_all(paths.runtime_root.join("python_embeded")).unwrap();
        std::fs::create_dir_all(paths.runtime_root.join("ComfyUI")).unwrap();
        std::fs::create_dir_all(&paths.data_root).unwrap();
        std::fs::create_dir_all(&paths.models_root).unwrap();
        std::fs::write(
            paths.runtime_root.join("python_embeded/python.exe"),
            b"fixture",
        )
        .unwrap();
        std::fs::write(paths.runtime_root.join("ComfyUI/main.py"), b"fixture").unwrap();

        let spec = fixed_comfyui_process_spec(&paths, 49152).unwrap();
        assert!(spec
            .arguments
            .windows(2)
            .any(|values| values == ["--listen", "127.0.0.1"]));
        assert!(spec
            .arguments
            .contains(&"--disable-all-custom-nodes".into()));
        assert!(spec.arguments.contains(&"--disable-api-nodes".into()));
        assert!(!spec
            .arguments
            .iter()
            .any(|argument| argument.ends_with(".bat")));
        assert!(managed_comfyui_process_spec(&paths, 49152)
            .unwrap_err()
            .contains("disabled"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn job_owned_fixture_exits_and_captures_bounded_logs() {
        let executable = std::env::current_exe().unwrap();
        let working_directory = executable.parent().unwrap().to_path_buf();
        let spec = ProcessSpec {
            executable,
            arguments: vec!["--list".into()],
            working_directory,
        };
        let process = ManagedProcess::launch(&spec).unwrap();
        assert!(process
            .wait_for_exit(Duration::from_secs(20))
            .unwrap()
            .is_some());
        let (stdout, stderr) = process.log_tail();
        assert!(stdout.contains("tests::") || stderr.contains("tests::"));
    }

    #[test]
    fn owned_running_fixture_is_terminated_only_through_its_job() {
        let executable = std::env::current_exe().unwrap();
        let working_directory = executable.parent().unwrap().to_path_buf();
        let spec = ProcessSpec {
            executable,
            arguments: vec![
                "--ignored".into(),
                "--exact".into(),
                "image_runtime::windows_supervisor::tests::slow_child_fixture".into(),
                "--nocapture".into(),
            ],
            working_directory,
        };
        let mut process = ManagedProcess::launch(&spec).unwrap();
        assert!(process.is_running().unwrap());
        process.terminate_owned_tree().unwrap();
        assert!(process
            .wait_for_exit(Duration::from_secs(5))
            .unwrap()
            .is_some());
    }

    #[test]
    #[ignore = "synthetic child; launched by the ownership integration test"]
    fn slow_child_fixture() {
        std::thread::sleep(Duration::from_secs(30));
    }
}

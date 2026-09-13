import type { ProjectInfo } from '../types/project'
import { FileTree } from './FileTree'

export function ProjectSidebar({ project, onOpen, busy }: { project: ProjectInfo | null; onOpen: () => void; busy: boolean }) {
  return <aside className="sidebar">
    <div className="sidebar-heading"><span>PROJECT</span><button className="subtle-button" onClick={onOpen} disabled={busy} title="Open another project">Open folder</button></div>
    {project ? <>
      <div className="border-b border-stone-800 px-4 pb-4 pt-3">
        <h2 className="truncate text-sm font-semibold text-stone-100" title={project.name}>{project.name}</h2>
        <p className="mt-1 break-all text-xs leading-5 text-stone-500" title={project.path}>{project.path}</p>
        <div className="mt-4 space-y-2 text-xs"><div className="flex justify-between gap-2"><span className="text-stone-500">Repository</span><span className="truncate text-stone-300" title={project.repository?.name}>{project.repository?.name ?? 'No Git repository'}</span></div>{project.repository && <><div className="flex justify-between gap-2"><span className="text-stone-500">Branch</span><span className="truncate font-mono text-stone-300" title={project.repository.branch}>{project.repository.branch}</span></div><div className="flex justify-between gap-2"><span className="text-stone-500">Status</span><span className={project.repository.changedFiles ? 'text-amber-400' : 'text-emerald-400'}>{project.repository.changedFiles ? `${project.repository.changedFiles} changed ${project.repository.changedFiles === 1 ? 'file' : 'files'}` : 'Clean'}</span></div></>}</div>
      </div>
      <div className="px-4 pb-2 pt-4 text-[11px] font-semibold tracking-[0.14em] text-stone-500">FILES</div>
      <FileTree entries={project.tree} truncated={project.treeTruncated} />
    </> : <p className="px-4 pt-4 text-xs leading-5 text-stone-500">Open a local folder to explore its files and Git status.</p>}
  </aside>
}

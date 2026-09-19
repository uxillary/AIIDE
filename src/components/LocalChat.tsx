import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Elma, type ElmaState } from './Elma'
import { ImageResultCard } from './ImageResultCard'
import { getAgentDebugStatus, getLatestAgentTrace, ollamaProvider, setAgentDebug } from '../services/ai/ollama'
import { comfyUiProvider } from '../services/image/comfyui'
import type { ChatMessage, PendingProposal, ProviderStatus } from '../types/ai'
import type { ImageEngineStatus, ImageJob } from '../types/image'

const MODEL_KEY = 'aiide.selected-model'
const INSPECTION_DISPLAY_MS = 700
const SUCCESS_DISPLAY_MS = 1300
const ERROR_DISPLAY_MS = 2000
const IMAGE_POLL_MS = 1200
const STARTER_PROMPTS = ['Tell me about this project', 'Find where this is implemented', 'Make a small change']

type ImageOutcome = { kind: 'saved'; path: string } | { kind: 'rejected' } | null
interface ConversationMessage extends ChatMessage {
  conversationId?: string
  channel?: 'chat' | 'image'
  imageJob?: ImageJob
  imageOutcome?: ImageOutcome
}

function messageOf(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'The local request failed.'
}

function imageActive(job: ImageJob) {
  return job.status === 'submitting' || job.status === 'queued' || job.status === 'generating'
}

export function LocalChat({ projectOpen, projectPath, projectBusy, onOpenProject, onProposal, changeState }: { projectOpen: boolean; projectPath?: string; projectBusy: boolean; onOpenProject: () => void; onProposal: (proposal: PendingProposal) => void; changeState: 'working' | 'success' | 'error' | null }) {
  const [mode, setMode] = useState<'chat' | 'image'>('chat')
  const [status, setStatus] = useState<ProviderStatus | null>(null)
  const [imageEngine, setImageEngine] = useState<ImageEngineStatus | null>(null)
  const [selectedModel, setSelectedModel] = useState('')
  const [messages, setMessages] = useState<ConversationMessage[]>([])
  const [prompt, setPrompt] = useState('')
  const [loading, setLoading] = useState(false)
  const [imageSubmitting, setImageSubmitting] = useState(false)
  const [imageActionJob, setImageActionJob] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const [checkingImage, setCheckingImage] = useState(false)
  const [activeSteps, setActiveSteps] = useState<string[]>([])
  const [retrying, setRetrying] = useState(false)
  const [inspecting, setInspecting] = useState(false)
  const [terminalState, setTerminalState] = useState<'success' | 'error' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [debug, setDebug] = useState(false)
  const [hasTrace, setHasTrace] = useState(false)
  const [debugTrace, setDebugTrace] = useState<string | null>(null)
  const [copyStatus, setCopyStatus] = useState('')
  const [previewUrls, setPreviewUrls] = useState<Record<string, string>>({})
  const previewUrlsRef = useRef<Record<string, string>>({})
  const projectPathRef = useRef(projectPath)
  const conversationSequenceRef = useRef(0)
  const endRef = useRef<HTMLDivElement>(null)
  const checkingRef = useRef(false)
  const checkingImageRef = useRef(false)
  const inspectionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const terminalTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  function showTerminalState(state: 'success' | 'error', duration: number) {
    if (terminalTimerRef.current) clearTimeout(terminalTimerRef.current)
    setTerminalState(state)
    terminalTimerRef.current = setTimeout(() => setTerminalState(null), duration)
  }

  async function refresh() {
    if (checkingRef.current) return
    checkingRef.current = true
    setChecking(true)
    try {
      const next = await ollamaProvider.getStatus()
      setStatus(next)
      setSelectedModel(current => {
        const saved = localStorage.getItem(MODEL_KEY)
        return next.models.find(model => model.id === current)?.id
          ?? next.models.find(model => model.id === saved)?.id
          ?? next.models[0]?.id ?? ''
      })
    } catch (cause) {
      setStatus({ state: 'offline', models: [], error: { code: 'unavailable', message: messageOf(cause) } })
    } finally { checkingRef.current = false; setChecking(false) }
  }

  async function refreshImage() {
    if (checkingImageRef.current) return
    checkingImageRef.current = true
    setCheckingImage(true)
    try { setImageEngine(await comfyUiProvider.getStatus()) }
    catch (cause) { setImageEngine({ state: 'offline', endpoint: 'http://127.0.0.1:8188', checkpoint: 'sd_xl_base_1.0.safetensors', busy: false, error: messageOf(cause) }) }
    finally { checkingImageRef.current = false; setCheckingImage(false) }
  }

  function updateImageJob(next: ImageJob) {
    setMessages(current => current.map(message => message.imageJob?.jobId === next.jobId ? { ...message, imageJob: next } : message))
  }

  function setImageOutcome(jobId: string, outcome: ImageOutcome) {
    setMessages(current => current.map(message => message.imageJob?.jobId === jobId ? { ...message, imageOutcome: outcome } : message))
  }

  function rememberPreview(jobId: string, url: string) {
    const previous = previewUrlsRef.current[jobId]
    if (previous) URL.revokeObjectURL(previous)
    previewUrlsRef.current = { ...previewUrlsRef.current, [jobId]: url }
    setPreviewUrls(previewUrlsRef.current)
  }

  useEffect(() => { void refresh(); void getAgentDebugStatus().then(value => { setDebug(value.enabled); setHasTrace(value.hasTrace) }) }, [])
  useEffect(() => { if (mode === 'image' && !imageEngine) void refreshImage() }, [mode, imageEngine])
  useEffect(() => {
    if (projectPathRef.current && projectPathRef.current !== projectPath) {
      setMessages(current => current.map(message => message.imageJob && !message.imageOutcome && !imageActive(message.imageJob) ? { ...message, imageOutcome: { kind: 'rejected' } } : message))
    }
    projectPathRef.current = projectPath
  }, [projectPath])
  useEffect(() => { const subscription = listen('repository-inspection-start', () => {
    setRetrying(false)
    setInspecting(true)
    if (inspectionTimerRef.current) clearTimeout(inspectionTimerRef.current)
  }); return () => { void subscription.then(unlisten => unlisten()) } }, [])
  useEffect(() => { const subscription = listen<{ label: string }>('repository-activity', event => {
    setRetrying(false)
    setInspecting(true)
    setActiveSteps(current => [...current, event.payload.label])
    if (inspectionTimerRef.current) clearTimeout(inspectionTimerRef.current)
    inspectionTimerRef.current = setTimeout(() => setInspecting(false), INSPECTION_DISPLAY_MS)
  }); return () => { void subscription.then(unlisten => unlisten()) } }, [])
  useEffect(() => { const subscription = listen('repository-retry', () => { setInspecting(false); setRetrying(true) }); return () => { void subscription.then(unlisten => unlisten()) } }, [])
  useEffect(() => { const subscription = listen('proposal-validation-start', () => { setInspecting(false); setRetrying(true) }); return () => { void subscription.then(unlisten => unlisten()) } }, [])
  useEffect(() => () => {
    if (inspectionTimerRef.current) clearTimeout(inspectionTimerRef.current)
    if (terminalTimerRef.current) clearTimeout(terminalTimerRef.current)
    Object.values(previewUrlsRef.current).forEach(url => URL.revokeObjectURL(url))
  }, [])
  useEffect(() => { endRef.current?.scrollIntoView({ behavior: 'smooth' }) }, [messages, loading, imageSubmitting])

  const activeImage = [...messages].reverse().find(message => message.imageJob && !message.imageOutcome && imageActive(message.imageJob))?.imageJob
  useEffect(() => {
    if (!activeImage) return
    let stopped = false
    const timer = setTimeout(() => {
      void comfyUiProvider.getJob(activeImage.jobId).then(job => { if (!stopped) updateImageJob(job) })
        .catch(cause => { if (!stopped) setError(messageOf(cause)) })
    }, IMAGE_POLL_MS)
    return () => { stopped = true; clearTimeout(timer) }
  }, [activeImage])

  const previewPending = messages.find(message => message.imageJob?.status === 'ready' && message.imageJob.previewAvailable && !previewUrls[message.imageJob.jobId])?.imageJob
  useEffect(() => {
    if (!previewPending) return
    let stopped = false
    void comfyUiProvider.getPreview(previewPending.jobId).then(bytes => {
      if (stopped) return
      rememberPreview(previewPending.jobId, URL.createObjectURL(new Blob([bytes], { type: 'image/png' })))
    }).catch(cause => { if (!stopped) setError(messageOf(cause)) })
    return () => { stopped = true }
  }, [previewPending])

  async function sendChat() {
    const content = prompt.trim()
    if (!content || !selectedModel || loading || imageSubmitting || activeImage || status?.state !== 'connected') return
    const model = selectedModel
    const previous = messages
    const nextMessages: ConversationMessage[] = [...messages, { role: 'user', content, channel: 'chat' }]
    setMessages(nextMessages)
    setPrompt('')
    setLoading(true)
    setActiveSteps([])
    setRetrying(false)
    setInspecting(false)
    setTerminalState(null)
    if (inspectionTimerRef.current) clearTimeout(inspectionTimerRef.current)
    if (terminalTimerRef.current) clearTimeout(terminalTimerRef.current)
    setError(null)
    try {
      const chatMessages = nextMessages.filter(message => message.channel !== 'image' && !message.imageJob)
      const response = await ollamaProvider.chat({ model, messages: chatMessages.slice(-39).map(({ role, content }) => ({ role, content })) })
      setMessages(current => [...current, { role: 'assistant', content: response.content, model: response.model, activity: response.activity, channel: 'chat' }])
      if (response.proposal) onProposal(response.proposal)
      showTerminalState('success', SUCCESS_DISPLAY_MS)
    } catch (cause) {
      setError(messageOf(cause))
      setPrompt(content)
      setMessages(previous)
      showTerminalState('error', ERROR_DISPLAY_MS)
      await refresh()
    } finally {
      if (debug) { try { const state = await getAgentDebugStatus(); setHasTrace(state.hasTrace); setDebugTrace(null) } catch { setHasTrace(false) } }
      setLoading(false); setInspecting(false); setRetrying(false)
    }
  }

  async function startImagePrompt(content: string) {
    conversationSequenceRef.current += 1
    const conversationId = `image-request-${conversationSequenceRef.current}`
    setMessages(current => [...current, { role: 'user', content, channel: 'image', conversationId }])
    setPrompt('')
    setImageSubmitting(true)
    setError(null)
    try {
      const engine = await comfyUiProvider.getStatus()
      setImageEngine(engine)
      if (engine.state !== 'connected') throw new Error(engine.error ?? 'ComfyUI is unavailable at the configured local endpoint.')
      const job = await comfyUiProvider.start(content)
      setMessages(current => [...current, { role: 'assistant', content: '', channel: 'image', imageJob: job, imageOutcome: null }])
    } catch (cause) {
      setMessages(current => current.filter(message => message.conversationId !== conversationId))
      setPrompt(content)
      setError(messageOf(cause))
    } finally { setImageSubmitting(false) }
  }

  async function sendImage() {
    const content = prompt.trim()
    if (!content || loading || imageSubmitting || activeImage) return
    await startImagePrompt(content)
  }

  async function cancelImage(job: ImageJob) {
    setImageActionJob(job.jobId); setError(null)
    try { updateImageJob(await comfyUiProvider.cancel(job.jobId)) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setImageActionJob(null) }
  }

  async function rejectImage(job: ImageJob) {
    setImageActionJob(job.jobId); setError(null)
    try { await comfyUiProvider.reject(job.jobId); setImageOutcome(job.jobId, { kind: 'rejected' }) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setImageActionJob(null) }
  }

  async function regenerateImage(job: ImageJob) {
    setImageActionJob(job.jobId); setError(null)
    try {
      await comfyUiProvider.reject(job.jobId)
      setImageOutcome(job.jobId, { kind: 'rejected' })
      setImageActionJob(null)
      await startImagePrompt(job.prompt)
    } catch (cause) { setError(messageOf(cause)); setImageActionJob(null) }
  }

  async function saveImage(job: ImageJob, relativePath: string) {
    setImageActionJob(job.jobId); setError(null)
    try { setImageOutcome(job.jobId, { kind: 'saved', path: await comfyUiProvider.save(job.jobId, relativePath) }) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setImageActionJob(null) }
  }

  async function toggleDebug() {
    const state = await setAgentDebug(!debug)
    setDebug(state.enabled); setHasTrace(state.hasTrace); setDebugTrace(null); setCopyStatus('')
  }

  async function loadTrace() {
    const trace = await getLatestAgentTrace(); setDebugTrace(trace); return trace
  }

  async function copyTrace() {
    const trace = await loadTrace()
    if (!trace) return
    try { await navigator.clipboard.writeText(trace); setCopyStatus('✓ Copied') }
    catch { setCopyStatus('Copy failed') }
  }

  const connected = status?.state === 'connected'
  const chatReady = projectOpen && connected && status.models.length > 0 && !activeImage
  const openImage = [...messages].reverse().find(message => message.imageJob && !message.imageOutcome)?.imageJob
  const imageReady = projectOpen && imageEngine?.state === 'connected' && !openImage && !loading
  const ready = mode === 'chat' ? chatReady : imageReady
  const working = loading || imageSubmitting || Boolean(activeImage)
  const elmaState: ElmaState = changeState ?? terminalState ?? (working ? inspecting ? 'inspecting' : retrying ? 'working' : imageSubmitting || activeImage ? 'working' : 'thinking' : 'idle')
  const elmaStatus = changeState === 'working' ? 'Applying the approved change…'
    : changeState === 'success' ? 'Change applied'
      : changeState === 'error' || terminalState === 'error' ? 'Something went wrong'
        : terminalState === 'success' ? 'Done'
          : imageSubmitting ? 'Submitting image workflow…'
            : activeImage ? activeImage.statusLabel
              : loading && inspecting ? 'Checking files…'
                : loading && retrying ? 'Working on it…'
                  : loading ? 'Thinking…' : 'Ready'
  const currentActivity = activeSteps.at(-1)
  const showElmaStatus = messages.length > 0 || working || Boolean(changeState) || Boolean(terminalState)
  const placeholder = mode === 'image'
    ? imageReady ? 'Describe an image to generate locally…' : imageEngine?.error ?? 'Connect ComfyUI to generate images'
    : chatReady ? 'Ask Elma about this project…' : 'Connect Ollama and select a model to chat'

  return <main className="main-panel">
    <div className="main-label">ELMA · LOCAL AI</div>
    <div className="chat-toolbar">
      {mode === 'chat' ? <>
        <div className="provider-state"><span className={`connection-dot ${connected ? 'connected' : ''}`} /><span>{ollamaProvider.name}</span><span className="connection-state">{checking ? 'Checking…' : connected ? 'Connected' : status?.state === 'error' ? 'Error' : 'Offline'}</span></div>
        <div className="workspace-controls"><label htmlFor="model">Model</label><select id="model" className="model-select" value={selectedModel} disabled={!chatReady || loading} onChange={event => { setSelectedModel(event.target.value); localStorage.setItem(MODEL_KEY, event.target.value) }}><option value="">{connected && status.models.length ? 'Select model' : 'No models'}</option>{status?.models.map(model => <option key={model.id} value={model.id}>{model.name} — {model.profile.label}</option>)}</select><button className="small-button" aria-pressed={debug} disabled={loading} onClick={() => void toggleDebug()}><span aria-hidden="true">›_</span> Debug {debug ? 'on' : 'off'}</button><button className="icon-button" aria-label="Retry Ollama connection" title="Retry connection" onClick={() => void refresh()} disabled={checking || loading}>↻</button></div>
      </> : <>
        <div className="provider-state"><span className={`connection-dot ${imageEngine?.state === 'connected' ? 'connected' : ''}`} /><span>{comfyUiProvider.name}</span><span className="connection-state">{checkingImage ? 'Checking…' : imageEngine?.state === 'connected' ? 'Connected' : imageEngine?.state === 'error' ? 'Error' : 'Offline'}</span></div>
        <div className="workspace-controls"><span className="image-model-label">SDXL 1.0 · {imageEngine?.checkpoint ?? 'sd_xl_base_1.0.safetensors'}</span><button className="icon-button" aria-label="Retry ComfyUI connection" title="Retry connection" onClick={() => void refreshImage()} disabled={checkingImage || imageSubmitting}>↻</button></div>
      </>}
    </div>
    {mode === 'chat' && debug && <details className="diagnostics" onToggle={event => { if (event.currentTarget.open && hasTrace && !debugTrace) void loadTrace() }}><summary><span aria-hidden="true">›_</span> Agent diagnostics</summary><div className="diagnostics-tools"><button className="small-button" disabled={!hasTrace} onClick={() => void copyTrace()}><span aria-hidden="true">⧉</span> Copy trace</button>{copyStatus && <span role="status" className={copyStatus === 'Copy failed' ? 'copy-error' : 'copy-success'}>{copyStatus}</span>}<span className="diagnostics-note">Local traces may contain prompts, paths, and source context. Review before sharing.</span></div>{debugTrace && <pre className="debug-trace">{debugTrace}</pre>}</details>}
    {projectOpen && <div className="chat-notice">{mode === 'image' ? 'Generated images stay temporary until you approve a project-relative save.' : 'Elma has controlled read-only access to this project.'}</div>}
    {showElmaStatus && <div className="elma-status"><Elma state={elmaState} size="presence" /><div><strong>{elmaStatus}</strong>{loading && currentActivity && <span>{currentActivity}</span>}</div></div>}
    <div className="chat-history">
      {!projectOpen && messages.length === 0 && <div className="chat-empty welcome-state"><Elma state="idle" size="hero" /><span className="eyebrow">LOCAL-FIRST CODING COMPANION</span><h2>Open a project and we'll take a look</h2><p>Elma can inspect your project, prepare focused changes, or generate an image for you to review.</p><button className="primary-button" disabled={projectBusy} onClick={onOpenProject}><span aria-hidden="true">▣</span> {projectBusy ? 'Opening…' : 'Open project'}</button></div>}
      {projectOpen && mode === 'chat' && !status && <div className="chat-empty">Checking for a local Ollama service…</div>}
      {projectOpen && mode === 'chat' && status?.state === 'offline' && <div className="chat-empty"><h2>Ollama not detected</h2><p>Start your local Ollama service, then try again.</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && mode === 'chat' && status?.state === 'error' && <div className="chat-empty"><h2>Ollama connection issue</h2><p>{status.error?.message}</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && mode === 'chat' && connected && !status.models.length && <div className="chat-empty"><h2>No local models installed</h2><p>Ollama is connected, but no local models are installed. Pull a model with the Ollama CLI, then retry.</p></div>}
      {projectOpen && mode === 'image' && imageEngine?.state !== 'connected' && messages.length === 0 && <div className="chat-empty"><h2>ComfyUI not detected</h2><p>{imageEngine?.error ?? 'Start ComfyUI with the SDXL checkpoint installed, then try again.'}</p><button className="primary-button" onClick={() => void refreshImage()} disabled={checkingImage}>Retry</button></div>}
      {projectOpen && mode === 'chat' && chatReady && messages.length === 0 && <div className="chat-empty project-start"><Elma state="idle" size="hero" /><span className="eyebrow">READY TO HAVE A LOOK</span><h2>What are we working on?</h2><p>I can inspect this project, explain what I find, or prepare a focused edit for review.</p><div className="starter-prompts">{STARTER_PROMPTS.map(starter => <button key={starter} onClick={() => setPrompt(starter)}>{starter}<span aria-hidden="true">→</span></button>)}</div></div>}
      {projectOpen && mode === 'image' && imageReady && messages.length === 0 && <div className="chat-empty project-start"><Elma state="idle" size="hero" /><span className="eyebrow">LOCAL SDXL · REVIEW BEFORE SAVE</span><h2>What should we make?</h2><p>Describe one image. ComfyUI will generate a temporary 1024 × 1024 preview.</p></div>}
      {messages.map((message, index) => <div className={`chat-message chat-message-${message.role}`} key={message.imageJob?.jobId ?? index}><div className="chat-speaker"><span>{message.role === 'user' ? 'YOU' : 'ELMA'}</span>{message.role === 'assistant' && message.model && <small>{message.model}</small>}{message.channel === 'image' && <small>IMAGE</small>}</div>{Boolean(message.activity?.length) && <details className="message-activity"><summary>Inspected {message.activity?.length} items</summary><ul>{message.activity?.map((item, step) => <li key={step}>✓ {item.label}</li>)}</ul></details>}{message.content && <div className="message-body">{message.content}</div>}{message.imageJob && <ImageResultCard job={message.imageJob} previewUrl={previewUrls[message.imageJob.jobId]} outcome={message.imageOutcome ?? null} busy={imageActionJob === message.imageJob.jobId} onCancel={() => void cancelImage(message.imageJob!)} onReject={() => void rejectImage(message.imageJob!)} onRegenerate={() => void regenerateImage(message.imageJob!)} onSave={path => void saveImage(message.imageJob!, path)} />}</div>)}
      {loading && <div className="chat-message activity-message"><div>{retrying ? 'Working on it…' : inspecting ? 'Checking files…' : activeSteps.length ? 'Preparing an answer…' : 'Thinking…'}</div>{activeSteps.map((step, index) => <div className="activity-step" key={index}>✓ {step}</div>)}</div>}
      {imageSubmitting && <div className="chat-message activity-message">Submitting the fixed SDXL workflow to ComfyUI…</div>}
      <div ref={endRef} />
    </div>
    <div className="chat-composer">
      <div className="composer-mode" role="group" aria-label="Request type"><button aria-pressed={mode === 'chat'} disabled={loading || imageSubmitting} onClick={() => { setMode('chat'); setError(null) }}>Chat</button><button aria-pressed={mode === 'image'} disabled={loading || imageSubmitting} onClick={() => { setMode('image'); setError(null) }}>Image</button></div>
      {error && <div role="alert" className="composer-error">{error}</div>}
      <div className="composer-row"><textarea aria-label={mode === 'image' ? 'Image prompt' : 'Prompt'} className="prompt-input" value={prompt} disabled={!ready || loading || imageSubmitting} maxLength={mode === 'image' ? 4000 : 12000} placeholder={placeholder} onChange={event => setPrompt(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void (mode === 'image' ? sendImage() : sendChat()) } }} /><button className="primary-button send-button" onClick={() => void (mode === 'image' ? sendImage() : sendChat())} disabled={!ready || !prompt.trim() || loading || imageSubmitting}>{mode === 'image' ? 'Generate' : 'Send'}</button></div>
      <p className="composer-hint"><kbd>Enter</kbd> {mode === 'image' ? 'generate' : 'send'} <span>·</span> <kbd>Shift</kbd> + <kbd>Enter</kbd> new line <span>·</span> {mode === 'image' ? 'One local GPU job at a time' : 'Session-only chat'}</p>
    </div>
  </main>
}

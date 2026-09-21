import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { save } from '@tauri-apps/plugin-dialog'
import { ElmaPresence, type ElmaPresentation } from './Elma'
import { ImageResultCard } from './ImageResultCard'
import { ImageSetupCard } from './ImageSetupCard'
import { getAgentDebugStatus, getLatestAgentTrace, ollamaProvider, setAgentDebug } from '../services/ai/ollama'
import { comfyUiProvider } from '../services/image/comfyui'
import type { ChatMessage, PendingProposal, ProviderStatus } from '../types/ai'
import type { ImageEngineStatus, ImageJob } from '../types/image'

const MODEL_KEY = 'aiide.selected-model'
const MODE_KEY = 'aiide.composer-mode'
const CHAT_DRAFT_KEY = 'aiide.chat-draft'
const IMAGE_DRAFT_KEY = 'aiide.image-draft'
const IMAGE_HISTORY_LIMIT = 8
const INSPECTION_DISPLAY_MS = 700
const SUCCESS_DISPLAY_MS = 1300
const ERROR_DISPLAY_MS = 2000
const IMAGE_POLL_MS = 1200
const STARTER_PROMPTS = ['Tell me about this project', 'Find where this is implemented', 'Make a small change']
const FALLBACK_IMAGE_MODEL = {
  id: 'sdxl-1.0-base',
  displayName: 'SDXL 1.0 Base',
  architecture: 'SDXL',
  capabilities: ['text-to-image'],
  engineRequirement: 'ComfyUI core workflow API',
  workflowId: 'comfyui-sdxl-base-v1',
  checkpoint: 'sd_xl_base_1.0.safetensors',
  supportingFiles: [],
  requiredNodes: ['KSampler', 'CheckpointLoaderSimple', 'EmptyLatentImage', 'CLIPTextEncode', 'VAEDecode', 'PreviewImage'],
  hardwareGuidance: '8 GB VRAM is the supported acceptance-test floor; hardware detection is advisory.',
  license: 'CreativeML Open RAIL++-M',
  acquisition: 'User-provided external ComfyUI checkpoint; AIIDE does not download it in Stage B.',
}

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

function imageConnectionLabel(status: ImageEngineStatus | null, checking: boolean) {
  if (checking) return 'Checking…'
  switch (status?.state) {
    case 'ready': return 'Ready'
    case 'busy': return 'Busy'
    case 'managed_not_installed': return 'Not installed'
    case 'managed_installed': return 'Installed'
    case 'starting': return 'Starting'
    case 'failed': return 'Failed'
    case 'incompatible': return 'Incompatible'
    default: return status?.engineStatus === 'reachable' ? 'Reachable' : 'Offline'
  }
}

function unavailableImageStatus(message: string): ImageEngineStatus {
  return {
    state: 'unavailable', ready: false, endpoint: 'http://127.0.0.1:8188', model: FALLBACK_IMAGE_MODEL,
    ownershipMode: 'external', managedState: 'disabled', managedAcquisitionEnabled: false,
    managedMessage: 'Managed ComfyUI acquisition and execution are disabled pending approval.',
    checkpoint: FALLBACK_IMAGE_MODEL.checkpoint, busy: false, engineStatus: 'unavailable', modelStatus: 'unknown',
    checkpoints: [],
    hardwareStatus: 'unavailable', missingNodes: [], missingFiles: [], hardwareMessage: null, error: message,
  }
}

export function LocalChat({ projectOpen, projectBusy, onOpenProject, onProposal, onChangePreparation, onElmaPresentationChange, primaryElma }: { projectOpen: boolean; projectBusy: boolean; onOpenProject: () => void; onProposal: (proposal: PendingProposal) => void; onChangePreparation: (active: boolean) => void; onElmaPresentationChange: (presentation: ElmaPresentation) => void; primaryElma: ({ presentation: ElmaPresentation; animation?: Parameters<typeof ElmaPresence>[0]['animation']; onAnimationComplete?: () => void }) | null }) {
  const [mode, setMode] = useState<'chat' | 'image'>(() => localStorage.getItem(MODE_KEY) === 'image' ? 'image' : 'chat')
  const [status, setStatus] = useState<ProviderStatus | null>(null)
  const [imageEngine, setImageEngine] = useState<ImageEngineStatus | null>(null)
  const [selectedModel, setSelectedModel] = useState('')
  const [messages, setMessages] = useState<ConversationMessage[]>([])
  const [drafts, setDrafts] = useState(() => ({ chat: localStorage.getItem(CHAT_DRAFT_KEY) ?? '', image: localStorage.getItem(IMAGE_DRAFT_KEY) ?? '' }))
  const [loading, setLoading] = useState(false)
  const [imageSubmitting, setImageSubmitting] = useState(false)
  const [imageActionJob, setImageActionJob] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const [checkingImage, setCheckingImage] = useState(false)
  const [configuringImage, setConfiguringImage] = useState(false)
  const [imageSetupDismissed, setImageSetupDismissed] = useState(true)
  const [activeSteps, setActiveSteps] = useState<string[]>([])
  const [retrying, setRetrying] = useState(false)
  const [inspecting, setInspecting] = useState(false)
  const [terminalState, setTerminalState] = useState<'success' | 'error' | 'image-complete' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [debug, setDebug] = useState(false)
  const [hasTrace, setHasTrace] = useState(false)
  const [debugTrace, setDebugTrace] = useState<string | null>(null)
  const [copyStatus, setCopyStatus] = useState('')
  const [previewUrls, setPreviewUrls] = useState<Record<string, string>>({})
  const previewUrlsRef = useRef<Record<string, string>>({})
  const conversationSequenceRef = useRef(0)
  const promptRef = useRef<HTMLTextAreaElement>(null)
  const chatSubmitRef = useRef(false)
  const imageSubmitRef = useRef(false)
  const endRef = useRef<HTMLDivElement>(null)
  const checkingRef = useRef(false)
  const checkingImageRef = useRef(false)
  const inspectionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const terminalTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const prompt = drafts[mode]

  function setPrompt(value: string) {
    setDrafts(current => ({ ...current, [mode]: value }))
  }

  function setDraft(channel: 'chat' | 'image', value: string) {
    setDrafts(current => ({ ...current, [channel]: value }))
  }

  function restoreDraft(channel: 'chat' | 'image', value: string) {
    setDrafts(current => ({ ...current, [channel]: current[channel].trim() ? `${value}\n${current[channel]}` : value }))
  }

  function selectMode(next: 'chat' | 'image') {
    setMode(next)
    setError(null)
    if (next === 'image' && imageEngine && !imageEngine.ready) setImageSetupDismissed(false)
  }

  const showTerminalState = useCallback((state: 'success' | 'error' | 'image-complete', duration: number) => {
    if (terminalTimerRef.current) clearTimeout(terminalTimerRef.current)
    setTerminalState(state)
    terminalTimerRef.current = setTimeout(() => setTerminalState(null), duration)
  }, [])

  const refresh = useCallback(async () => {
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
  }, [])

  const refreshImage = useCallback(async () => {
    if (checkingImageRef.current) return
    checkingImageRef.current = true
    setCheckingImage(true)
    try {
      const next = await comfyUiProvider.getStatus()
      setImageEngine(next)
      if (!next.ready) setImageSetupDismissed(false)
    }
    catch (cause) {
      const message = messageOf(cause)
      setImageEngine(current => current ? { ...current, state: 'unavailable', ready: false, engineStatus: 'unavailable', error: message } : unavailableImageStatus(message))
      setImageSetupDismissed(false)
    }
    finally { checkingImageRef.current = false; setCheckingImage(false) }
  }, [])

  async function configureImage(endpoint: string, modelId: string, checkpoint: string) {
    setConfiguringImage(true)
    setError(null)
    try {
      const next = await comfyUiProvider.configure(endpoint, modelId, checkpoint)
      setImageEngine(next)
      setImageSetupDismissed(next.ready)
    } catch (cause) {
      setError(messageOf(cause))
    } finally { setConfiguringImage(false) }
  }

  const updateImageJob = useCallback((next: ImageJob) => {
    setMessages(current => current.map(message => message.imageJob?.jobId === next.jobId ? { ...message, imageJob: next } : message))
    setImageEngine(current => current ? {
      ...current,
      ready: true,
      busy: imageActive(next),
      state: imageActive(next) ? 'busy' : 'ready',
      engineStatus: imageActive(next) ? 'busy' : 'reachable',
      error: null,
    } : current)
    if (next.status === 'ready') showTerminalState('image-complete', SUCCESS_DISPLAY_MS)
    if (next.status === 'failed') showTerminalState('error', ERROR_DISPLAY_MS)
  }, [showTerminalState])

  function appendImageJob(job: ImageJob) {
    setImageEngine(current => current ? { ...current, busy: true, state: 'busy', engineStatus: 'busy' } : current)
    setMessages(current => {
      const existing = current.filter(message => message.imageJob).map(message => message.imageJob!.jobId)
      if (existing.length < IMAGE_HISTORY_LIMIT) return [...current, { role: 'assistant', content: '', channel: 'image', imageJob: job, imageOutcome: null }]
      const evicted = existing[0]
      const url = previewUrlsRef.current[evicted]
      if (url) URL.revokeObjectURL(url)
      const nextUrls = { ...previewUrlsRef.current }
      delete nextUrls[evicted]
      previewUrlsRef.current = nextUrls
      setPreviewUrls(nextUrls)
      return [...current.filter(message => message.imageJob?.jobId !== evicted), { role: 'assistant', content: '', channel: 'image', imageJob: job, imageOutcome: null }]
    })
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

  useEffect(() => { void refresh(); void refreshImage(); void getAgentDebugStatus().then(value => { setDebug(value.enabled); setHasTrace(value.hasTrace) }) }, [refresh, refreshImage])
  useEffect(() => { localStorage.setItem(MODE_KEY, mode); requestAnimationFrame(() => promptRef.current?.focus()) }, [mode])
  useEffect(() => { localStorage.setItem(CHAT_DRAFT_KEY, drafts.chat); localStorage.setItem(IMAGE_DRAFT_KEY, drafts.image) }, [drafts])
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
  useEffect(() => { const subscription = listen('proposal-validation-start', () => { setInspecting(false); setRetrying(true); onChangePreparation(true) }); return () => { void subscription.then(unlisten => unlisten()) } }, [onChangePreparation])
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
    let inFlight = false
    const poll = () => {
      if (inFlight) return
      inFlight = true
      void comfyUiProvider.getJob(activeImage.jobId).then(job => { if (!stopped) updateImageJob(job) })
        .catch(cause => {
          if (!stopped) {
            const message = messageOf(cause)
            setError(message)
            showTerminalState('error', ERROR_DISPLAY_MS)
            setImageEngine(current => current ? { ...current, state: 'unavailable', ready: false, engineStatus: 'unavailable', error: message } : current)
          }
        }).finally(() => { inFlight = false })
    }
    const timer = setInterval(poll, IMAGE_POLL_MS)
    return () => { stopped = true; clearInterval(timer) }
  }, [activeImage, showTerminalState, updateImageJob])

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
    const content = drafts.chat.trim()
    if (!content || !selectedModel || chatSubmitRef.current || loading || status?.state !== 'connected') return
    chatSubmitRef.current = true
    const model = selectedModel
    conversationSequenceRef.current += 1
    const conversationId = `chat-request-${conversationSequenceRef.current}`
    const nextMessages: ConversationMessage[] = [...messages, { role: 'user', content, channel: 'chat', conversationId }]
    setMessages(nextMessages)
    setDraft('chat', '')
    setLoading(true)
    setActiveSteps([])
    setRetrying(false)
    setInspecting(false)
    setTerminalState(null)
    onChangePreparation(false)
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
      restoreDraft('chat', content)
      setMessages(current => current.filter(message => message.conversationId !== conversationId))
      showTerminalState('error', ERROR_DISPLAY_MS)
      await refresh()
    } finally {
      if (debug) { try { const state = await getAgentDebugStatus(); setHasTrace(state.hasTrace); setDebugTrace(null) } catch { setHasTrace(false) } }
      chatSubmitRef.current = false; setLoading(false); setInspecting(false); setRetrying(false); onChangePreparation(false)
    }
  }

  async function startImagePrompt(content: string) {
    conversationSequenceRef.current += 1
    const conversationId = `image-request-${conversationSequenceRef.current}`
    setMessages(current => [...current, { role: 'user', content, channel: 'image', conversationId }])
    setDraft('image', '')
    setImageSubmitting(true)
    setError(null)
    try {
      const engine = await comfyUiProvider.getStatus()
      setImageEngine(engine)
      if (!engine.ready || engine.busy) throw new Error(engine.error ?? 'Image generation is not ready.')
      const job = await comfyUiProvider.start(content)
      appendImageJob(job)
    } catch (cause) {
      setMessages(current => current.filter(message => message.conversationId !== conversationId))
      restoreDraft('image', content)
      setError(messageOf(cause))
      showTerminalState('error', ERROR_DISPLAY_MS)
    } finally { setImageSubmitting(false) }
  }

  async function sendImage() {
    const content = drafts.image.trim()
    if (!content || loading || imageSubmitRef.current || imageSubmitting || activeImage) return
    imageSubmitRef.current = true
    try { await startImagePrompt(content) }
    finally { imageSubmitRef.current = false }
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
    if (imageSubmitRef.current || activeImage) return
    imageSubmitRef.current = true
    setImageActionJob(job.jobId); setImageSubmitting(true); setError(null)
    try {
      const regenerated = await comfyUiProvider.regenerate(job.jobId)
      setMessages(current => [...current, { role: 'user', content: job.prompt, channel: 'image' }])
      appendImageJob(regenerated)
    } catch (cause) { setError(messageOf(cause)); showTerminalState('error', ERROR_DISPLAY_MS) }
    finally { imageSubmitRef.current = false; setImageSubmitting(false); setImageActionJob(null) }
  }

  async function saveImage(job: ImageJob, relativePath: string) {
    setImageActionJob(job.jobId); setError(null)
    try { setImageOutcome(job.jobId, { kind: 'saved', path: await comfyUiProvider.save(job.jobId, relativePath) }) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setImageActionJob(null) }
  }

  async function saveImageAs(job: ImageJob) {
    const path = await save({ title: 'Save generated image', defaultPath: 'generated-image.png', filters: [{ name: 'PNG image', extensions: ['png'] }] })
    if (!path) return
    setImageActionJob(job.jobId); setError(null)
    try { setImageOutcome(job.jobId, { kind: 'saved', path: await comfyUiProvider.saveAs(job.jobId, path) }) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setImageActionJob(null) }
  }

  async function revealImage(path: string) {
    try { await comfyUiProvider.reveal(path) }
    catch (cause) { setError(messageOf(cause)) }
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
  const chatReady = projectOpen && connected && status.models.length > 0
  const imageReady = Boolean(imageEngine?.ready) && !imageEngine?.busy && !activeImage && !loading
  const imageNeedsAttention = Boolean(imageEngine && !imageEngine.ready)
  const ready = mode === 'chat' ? chatReady : imageReady
  const imagePreparing = imageSubmitting || activeImage?.status === 'submitting'
  const elmaState: ElmaPresentation['state'] = terminalState === 'image-complete' ? 'image-complete'
    : terminalState ?? (imagePreparing ? 'image-thinking' : activeImage ? 'image-creating' : loading ? inspecting ? 'inspecting' : 'thinking' : 'idle')
  const elmaStatus = terminalState === 'error' ? 'Something went wrong'
    : terminalState === 'success' ? 'Done'
      : terminalState === 'image-complete' ? 'Image complete'
        : imagePreparing ? 'Preparing the image workflow…'
          : activeImage ? activeImage.statusLabel
            : loading && inspecting ? 'Checking files…'
              : loading && retrying ? 'Working on it…'
                : loading ? 'Thinking…' : 'Ready'
  const currentActivity = activeSteps.at(-1)
  const elmaActivity = loading ? currentActivity : undefined
  useEffect(() => {
    onElmaPresentationChange({ state: elmaState, status: elmaStatus, activity: elmaActivity })
  }, [elmaState, elmaStatus, elmaActivity, onElmaPresentationChange])
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
        <div className="provider-state"><span className={`connection-dot ${imageEngine?.engineStatus === 'reachable' || imageEngine?.engineStatus === 'busy' ? 'connected' : ''}`} /><span>{comfyUiProvider.name}</span><span className="connection-state">{imageConnectionLabel(imageEngine, checkingImage)}</span></div>
        <div className="workspace-controls"><span className="image-model-label">{imageEngine?.model.displayName ?? 'SDXL 1.0 Base'} · {imageEngine?.checkpoint ?? 'sd_xl_base_1.0.safetensors'}</span><button className="small-button" onClick={() => setImageSetupDismissed(false)}>{imageNeedsAttention ? imageEngine?.engineStatus === 'unavailable' ? 'Connect / reconnect' : 'Configure' : 'Details'}</button><button className="icon-button" aria-label="Retry ComfyUI readiness" title="Retry readiness" onClick={() => void refreshImage()} disabled={checkingImage || imageSubmitting}>↻</button></div>
      </>}
    </div>
    {mode === 'chat' && debug && (
      <details
        className="diagnostics"
        onToggle={event => {
          if (event.currentTarget.open && hasTrace && !debugTrace) {
            void loadTrace()
          }
        }}
      >
        <summary>
          <span aria-hidden="true">›_</span> Agent diagnostics
        </summary>

        <div className="diagnostics-tools">
          <button
            className="small-button"
            disabled={!hasTrace}
            onClick={() => void copyTrace()}
          >
            <span aria-hidden="true">⧉</span> Copy trace
          </button>

          {copyStatus && (
            <span
              role="status"
              className={
                copyStatus === 'Copy failed'
                  ? 'copy-error'
                  : 'copy-success'
              }
            >
              {copyStatus}
            </span>
          )}

          <span className="diagnostics-note">
            Local traces may contain prompts, paths, and source context.
            Review before sharing.
          </span>
        </div>

        {debugTrace && (
          <pre className="debug-trace">{debugTrace}</pre>
        )}
      </details>
    )}

    {(projectOpen || mode === 'image') && (
      <div className="chat-notice">
        {mode === 'image'
          ? `Generated images stay temporary until you approve ${projectOpen ? 'a project save or Save As.' : 'a Save As destination.'}`
          : 'Elma can inspect your project and prepare changes. Files are modified only after you approve.'}
      </div>
    )}

    {primaryElma && <ElmaPresence {...primaryElma} />}
    <div className="chat-history">
      {!projectOpen && mode === 'chat' && messages.length === 0 && <div className="chat-empty welcome-state"><span className="eyebrow">LOCAL-FIRST CODING COMPANION</span><h2>Open a project and we'll take a look</h2><p>Elma can inspect your project, prepare focused changes, or switch to Image without opening a folder.</p><button className="primary-button" disabled={projectBusy} onClick={onOpenProject}><span aria-hidden="true">▣</span> {projectBusy ? 'Opening…' : 'Open project'}</button></div>}
      {projectOpen && mode === 'chat' && !status && <div className="chat-empty">Checking for a local Ollama service…</div>}
      {projectOpen && mode === 'chat' && status?.state === 'offline' && <div className="chat-empty"><h2>Ollama not detected</h2><p>Start your local Ollama service, then try again.</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && mode === 'chat' && status?.state === 'error' && <div className="chat-empty"><h2>Ollama connection issue</h2><p>{status.error?.message}</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && mode === 'chat' && connected && !status.models.length && <div className="chat-empty"><h2>No local models installed</h2><p>Ollama is connected, but no local models are installed. Pull a model with the Ollama CLI, then retry.</p></div>}
      {mode === 'image' && !imageEngine && <div className="chat-empty">Checking the ComfyUI engine and selected model…</div>}
      {mode === 'image' && imageEngine && !imageSetupDismissed && !activeImage && <ImageSetupCard status={imageEngine} checking={checkingImage} saving={configuringImage} onRetry={() => void refreshImage()} onConnect={configureImage} onDismiss={() => setImageSetupDismissed(true)} />}
      {projectOpen && mode === 'chat' && chatReady && messages.length === 0 && <div className="chat-empty project-start"><span className="eyebrow">READY TO HAVE A LOOK</span><h2>What are we working on?</h2><p>I can inspect this project, explain what I find, or prepare a focused edit for review.</p><div className="starter-prompts">{STARTER_PROMPTS.map(starter => <button key={starter} onClick={() => setPrompt(starter)}>{starter}<span aria-hidden="true">→</span></button>)}</div></div>}
      {mode === 'image' && imageReady && (!imageNeedsAttention || imageSetupDismissed) && messages.length === 0 && <div className="chat-empty project-start"><span className="eyebrow">LOCAL SDXL · REVIEW BEFORE SAVE</span><h2>What should we make?</h2><p>Describe one image. Readiness checks passed, but only a real GPU generation can confirm acceptance.</p></div>}
      {messages.map((message, index) => <div className={`chat-message chat-message-${message.role}`} key={message.imageJob?.jobId ?? index}><div className="chat-speaker"><span>{message.role === 'user' ? 'YOU' : 'ELMA'}</span>{message.role === 'assistant' && message.model && <small>{message.model}</small>}{message.channel === 'image' && <small>IMAGE</small>}</div>{Boolean(message.activity?.length) && <details className="message-activity"><summary>Inspected {message.activity?.length} items</summary><ul>{message.activity?.map((item, step) => <li key={step}>✓ {item.label}</li>)}</ul></details>}{message.content && <div className="message-body">{message.content}</div>}{message.imageJob && <ImageResultCard job={message.imageJob} previewUrl={previewUrls[message.imageJob.jobId]} outcome={message.imageOutcome ?? null} busy={imageActionJob === message.imageJob.jobId} projectOpen={projectOpen} onCancel={() => void cancelImage(message.imageJob!)} onReject={() => void rejectImage(message.imageJob!)} onRegenerate={() => void regenerateImage(message.imageJob!)} onSave={path => void saveImage(message.imageJob!, path)} onSaveAs={() => void saveImageAs(message.imageJob!)} onReveal={path => void revealImage(path)} />}</div>)}
      {loading && <div className="chat-message activity-message"><div>{retrying ? 'Working on it…' : inspecting ? 'Checking files…' : activeSteps.length ? 'Preparing an answer…' : 'Thinking…'}</div>{activeSteps.map((step, index) => <div className="activity-step" key={index}>✓ {step}</div>)}</div>}
      {imageSubmitting && <div className="chat-message activity-message">Submitting the fixed SDXL workflow to ComfyUI…</div>}
      <div ref={endRef} />
    </div>
    <div className="chat-composer">
      <div className="composer-mode" role="group" aria-label="Request type"><button aria-pressed={mode === 'chat'} onClick={() => selectMode('chat')}>Chat</button><button aria-pressed={mode === 'image'} onClick={() => selectMode('image')}>Image</button></div>
      {error && <div role="alert" className="composer-error">{error}</div>}
      <div className="composer-row"><textarea ref={promptRef} aria-label={mode === 'image' ? 'Image prompt' : 'Prompt'} className="prompt-input" value={prompt} maxLength={mode === 'image' ? 4000 : 12000} placeholder={placeholder} onChange={event => setPrompt(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void (mode === 'image' ? sendImage() : sendChat()) } }} /><button className="primary-button send-button" onClick={() => void (mode === 'image' ? sendImage() : sendChat())} disabled={!ready || !prompt.trim() || (mode === 'chat' ? loading : imageSubmitting)}>{mode === 'image' ? 'Generate' : 'Send'}</button></div>
      <p className="composer-hint"><kbd>Enter</kbd> {mode === 'image' ? 'generate' : 'send'} <span>·</span> <kbd>Shift</kbd> + <kbd>Enter</kbd> new line <span>·</span> {mode === 'image' ? 'One local GPU job at a time' : 'Session-only chat'}</p>
    </div>
  </main>
}

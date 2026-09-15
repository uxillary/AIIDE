import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Elma, type ElmaState } from './Elma'
import { getAgentDebugStatus, getLatestAgentTrace, ollamaProvider, setAgentDebug } from '../services/ai/ollama'
import type { ChatMessage, PendingProposal, ProviderStatus } from '../types/ai'

const MODEL_KEY = 'aiide.selected-model'
const INSPECTION_DISPLAY_MS = 700
const SUCCESS_DISPLAY_MS = 1300
const ERROR_DISPLAY_MS = 2000
const STARTER_PROMPTS = ['Tell me about this project', 'Find where this is implemented', 'Make a small change']

function messageOf(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'The local AI request failed.'
}

export function LocalChat({ projectOpen, projectBusy, onOpenProject, onProposal, changeState }: { projectOpen: boolean; projectBusy: boolean; onOpenProject: () => void; onProposal: (proposal: PendingProposal) => void; changeState: 'working' | 'success' | 'error' | null }) {
  const [status, setStatus] = useState<ProviderStatus | null>(null)
  const [selectedModel, setSelectedModel] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [prompt, setPrompt] = useState('')
  const [loading, setLoading] = useState(false)
  const [checking, setChecking] = useState(false)
  const [activeSteps, setActiveSteps] = useState<string[]>([])
  const [retrying, setRetrying] = useState(false)
  const [inspecting, setInspecting] = useState(false)
  const [terminalState, setTerminalState] = useState<'success' | 'error' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [debug, setDebug] = useState(false)
  const [hasTrace, setHasTrace] = useState(false)
  const [debugTrace, setDebugTrace] = useState<string | null>(null)
  const [copyStatus, setCopyStatus] = useState('')
  const endRef = useRef<HTMLDivElement>(null)
  const checkingRef = useRef(false)
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

  useEffect(() => { void refresh(); void getAgentDebugStatus().then(value => { setDebug(value.enabled); setHasTrace(value.hasTrace) }) }, [])
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
  }, [])
  useEffect(() => { endRef.current?.scrollIntoView({ behavior: 'smooth' }) }, [messages, loading])

  async function send() {
    const content = prompt.trim()
    if (!content || !selectedModel || loading || status?.state !== 'connected') return
    const model = selectedModel
    const nextMessages = [...messages, { role: 'user' as const, content }]
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
      const response = await ollamaProvider.chat({ model, messages: nextMessages.slice(-39).map(({ role, content }) => ({ role, content })) })
      setMessages(current => [...current, { role: 'assistant', content: response.content, model: response.model, activity: response.activity }])
      if (response.proposal) onProposal(response.proposal)
      showTerminalState('success', SUCCESS_DISPLAY_MS)
    } catch (cause) {
      setError(messageOf(cause))
      setPrompt(content)
      setMessages(messages)
      showTerminalState('error', ERROR_DISPLAY_MS)
      await refresh()
    } finally {
      if (debug) { try { const state = await getAgentDebugStatus(); setHasTrace(state.hasTrace); setDebugTrace(null) } catch { setHasTrace(false) } }
      setLoading(false); setInspecting(false); setRetrying(false)
    }
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
  const ready = connected && status.models.length > 0
  const elmaState: ElmaState = changeState ?? terminalState ?? (loading ? inspecting ? 'inspecting' : retrying ? 'working' : 'thinking' : 'idle')
  const elmaStatus = changeState === 'working' ? 'Applying the approved change…'
    : changeState === 'success' ? 'Change applied'
      : changeState === 'error' ? 'Something went wrong'
        : terminalState === 'error' ? 'Something went wrong'
    : terminalState === 'success' ? 'Done'
      : loading && inspecting ? 'Checking files…'
        : loading && retrying ? 'Working on it…'
          : loading ? 'Thinking…' : 'Ready'
  const currentActivity = activeSteps.at(-1)
  const showElmaStatus = messages.length > 0 || loading || Boolean(changeState) || Boolean(terminalState)
  return <main className="main-panel">
    <div className="main-label">ELMA · LOCAL AI</div>
    <div className="chat-toolbar">
      <div className="provider-state"><span className={`connection-dot ${connected ? 'connected' : ''}`} /><span>{ollamaProvider.name}</span><span className="connection-state">{checking ? 'Checking…' : connected ? 'Connected' : status?.state === 'error' ? 'Error' : 'Offline'}</span></div>
      <div className="workspace-controls"><label htmlFor="model">Model</label><select id="model" className="model-select" value={selectedModel} disabled={!ready || loading} onChange={event => { setSelectedModel(event.target.value); localStorage.setItem(MODEL_KEY, event.target.value) }}><option value="">{ready ? 'Select model' : 'No models'}</option>{status?.models.map(model => <option key={model.id} value={model.id}>{model.name}</option>)}</select><button className="small-button" aria-pressed={debug} disabled={loading} onClick={() => void toggleDebug()}><span aria-hidden="true">›_</span> Debug {debug ? 'on' : 'off'}</button><button className="icon-button" aria-label="Retry Ollama connection" title="Retry connection" onClick={() => void refresh()} disabled={checking || loading}>↻</button></div>
    </div>
    {debug && <details className="diagnostics" onToggle={event => { if (event.currentTarget.open && hasTrace && !debugTrace) void loadTrace() }}><summary><span aria-hidden="true">›_</span> Agent diagnostics</summary><div className="diagnostics-tools"><button className="small-button" disabled={!hasTrace} onClick={() => void copyTrace()}><span aria-hidden="true">⧉</span> Copy trace</button>{copyStatus && <span role="status" className={copyStatus === 'Copy failed' ? 'copy-error' : 'copy-success'}>{copyStatus}</span>}<span className="diagnostics-note">Local traces may contain prompts, paths, and source context. Review before sharing.</span></div>{debugTrace && <pre className="debug-trace">{debugTrace}</pre>}</details>}
    {projectOpen && <div className="chat-notice">Elma has controlled read-only access to this project.</div>}
    {showElmaStatus && <div className="elma-status"><Elma state={elmaState} /><div><strong>{elmaStatus}</strong>{loading && currentActivity && <span>{currentActivity}</span>}</div></div>}
    <div className="chat-history">
      {!projectOpen && messages.length === 0 && <div className="chat-empty welcome-state"><Elma state="idle" /><span className="eyebrow">LOCAL-FIRST CODING COMPANION</span><h2>Open a project and we’ll have a nosey 👀</h2><p>Elma can inspect your project and prepare focused changes for you to review.</p><button className="primary-button" disabled={projectBusy} onClick={onOpenProject}><span aria-hidden="true">▣</span> {projectBusy ? 'Opening…' : 'Open project'}</button></div>}
      {projectOpen && !status && <div className="chat-empty">Checking for a local Ollama service…</div>}
      {projectOpen && status?.state === 'offline' && <div className="chat-empty"><h2>Ollama not detected</h2><p>Start your local Ollama service, then try again.</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && status?.state === 'error' && <div className="chat-empty"><h2>Ollama connection issue</h2><p>{status.error?.message}</p><button className="primary-button" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {projectOpen && connected && !status.models.length && <div className="chat-empty"><h2>No local models installed</h2><p>Ollama is connected, but no local models are installed. Pull a model with the Ollama CLI, then retry.</p></div>}
      {projectOpen && ready && messages.length === 0 && <div className="chat-empty project-start"><Elma state="idle" /><span className="eyebrow">READY TO HAVE A LOOK</span><h2>What are we working on?</h2><p>I can inspect this project, explain what I find, or prepare a focused edit for review.</p><div className="starter-prompts">{STARTER_PROMPTS.map(starter => <button key={starter} onClick={() => setPrompt(starter)}>{starter}<span aria-hidden="true">→</span></button>)}</div></div>}
      {messages.map((message, index) => <div className="chat-message" key={index}><div className="chat-speaker">{message.role === 'user' ? 'YOU' : `ELMA · ${message.model}`}</div>{Boolean(message.activity?.length) && <details className="mb-3 text-xs text-stone-500"><summary>Inspected {message.activity?.length} items</summary><ul className="mt-2 space-y-1">{message.activity?.map((item, step) => <li key={step}>✓ {item.label}</li>)}</ul></details>}<div className="whitespace-pre-wrap break-words text-sm leading-6 text-stone-200">{message.content}</div></div>)}
      {loading && <div className="chat-message activity-message"><div>{retrying ? 'Working on it…' : inspecting ? 'Checking files…' : activeSteps.length ? 'Preparing an answer…' : 'Thinking…'}</div>{activeSteps.map((step, index) => <div className="activity-step" key={index}>✓ {step}</div>)}</div>}
      <div ref={endRef} />
    </div>
    <div className="chat-composer">{error && <div role="alert" className="mb-2 text-xs text-red-400">{error}</div>}<div className="flex items-end gap-3"><textarea aria-label="Prompt" className="prompt-input" value={prompt} disabled={!ready || loading} maxLength={12000} placeholder={ready ? 'Ask a question…' : 'Connect Ollama and select a model to chat'} onChange={event => setPrompt(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void send() } }} /><button className="primary-button" onClick={() => void send()} disabled={!ready || !prompt.trim() || loading}>Send</button></div><p className="mt-2 text-[11px] text-stone-600">Enter to send · Shift+Enter for a new line · Chat stays in this session</p></div>
  </main>
}

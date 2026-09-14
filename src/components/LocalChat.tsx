import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ollamaProvider } from '../services/ai/ollama'
import type { ChatMessage, ProviderStatus } from '../types/ai'

const MODEL_KEY = 'aiide.selected-model'

function messageOf(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'The local AI request failed.'
}

export function LocalChat({ projectOpen }: { projectOpen: boolean }) {
  const [status, setStatus] = useState<ProviderStatus | null>(null)
  const [selectedModel, setSelectedModel] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [prompt, setPrompt] = useState('')
  const [loading, setLoading] = useState(false)
  const [checking, setChecking] = useState(false)
  const [activeSteps, setActiveSteps] = useState<string[]>([])
  const [retrying, setRetrying] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const endRef = useRef<HTMLDivElement>(null)
  const checkingRef = useRef(false)

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

  useEffect(() => { void refresh() }, [])
  useEffect(() => { const subscription = listen<{ label: string }>('repository-activity', event => { setRetrying(false); setActiveSteps(current => [...current, event.payload.label]) }); return () => { void subscription.then(unlisten => unlisten()) } }, [])
  useEffect(() => { const subscription = listen('repository-retry', () => setRetrying(true)); return () => { void subscription.then(unlisten => unlisten()) } }, [])
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
    setError(null)
    try {
      const response = await ollamaProvider.chat({ model, messages: nextMessages.slice(-39).map(({ role, content }) => ({ role, content })) })
      setMessages(current => [...current, { role: 'assistant', content: response.content, model: response.model, activity: response.activity }])
    } catch (cause) {
      setError(messageOf(cause))
      setPrompt(content)
      setMessages(messages)
      await refresh()
    } finally { setLoading(false) }
  }

  const connected = status?.state === 'connected'
  const ready = connected && status.models.length > 0
  return <main className="main-panel">
    <div className="main-label">LOCAL AI</div>
    <div className="chat-toolbar">
      <div className="flex items-center gap-3"><span className="text-xs font-semibold text-stone-200">{ollamaProvider.name}</span><span className={`connection-state ${connected ? 'text-emerald-400' : status ? 'text-amber-400' : 'text-stone-500'}`}>{checking ? 'Checking…' : connected ? 'Connected' : status?.state === 'error' ? 'Error' : 'Offline'}</span></div>
      <div className="flex items-center gap-2"><label htmlFor="model" className="text-xs text-stone-500">Model</label><select id="model" className="model-select" value={selectedModel} disabled={!ready || loading} onChange={event => { setSelectedModel(event.target.value); localStorage.setItem(MODEL_KEY, event.target.value) }}><option value="">{ready ? 'Select model' : 'No models'}</option>{status?.models.map(model => <option key={model.id} value={model.id}>{model.name}</option>)}</select><button className="subtle-button" onClick={() => void refresh()} disabled={checking || loading}>Retry</button></div>
    </div>
    {projectOpen && <div className="chat-notice">Elma has controlled read-only access to this project.</div>}
    <div className="chat-history">
      {!status && <div className="chat-empty">Checking for a local Ollama service…</div>}
      {status?.state === 'offline' && <div className="chat-empty"><h2>Ollama not detected</h2><p>AIIDE uses a local Ollama service for offline AI. Start Ollama and try again.</p><button className="primary-button mt-5" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {status?.state === 'error' && <div className="chat-empty"><h2>Ollama connection issue</h2><p>{status.error?.message}</p><button className="primary-button mt-5" onClick={() => void refresh()} disabled={checking}>Retry</button></div>}
      {connected && !status.models.length && <div className="chat-empty"><h2>No local models installed</h2><p>Ollama is connected, but no local models are installed. Pull a model with the Ollama CLI, then select Retry.</p></div>}
      {ready && messages.length === 0 && <div className="chat-empty"><h2>Your local AI coding workspace.</h2><p>{projectOpen ? 'Ask Elma about the opened project.' : 'Ask a general question to start a local conversation.'}</p></div>}
      {messages.map((message, index) => <div className="chat-message" key={index}><div className="chat-speaker">{message.role === 'user' ? 'YOU' : `ELMA · ${message.model}`}</div>{Boolean(message.activity?.length) && <details className="mb-3 text-xs text-stone-500"><summary>Inspected {message.activity?.length} items</summary><ul className="mt-2 space-y-1">{message.activity?.map((item, step) => <li key={step}>✓ {item.label}</li>)}</ul></details>}<div className="whitespace-pre-wrap break-words text-sm leading-6 text-stone-200">{message.content}</div></div>)}
      {loading && <div className="chat-message text-xs text-stone-500"><div>{retrying ? 'Elma is retrying the local model…' : activeSteps.length ? 'Elma is preparing an answer…' : 'Elma is inspecting the request…'}</div>{activeSteps.map((step, index) => <div key={index}>✓ {step}</div>)}</div>}
      <div ref={endRef} />
    </div>
    <div className="chat-composer">{error && <div role="alert" className="mb-2 text-xs text-red-400">{error}</div>}<div className="flex items-end gap-3"><textarea aria-label="Prompt" className="prompt-input" value={prompt} disabled={!ready || loading} maxLength={12000} placeholder={ready ? 'Ask a question…' : 'Connect Ollama and select a model to chat'} onChange={event => setPrompt(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void send() } }} /><button className="primary-button" onClick={() => void send()} disabled={!ready || !prompt.trim() || loading}>Send</button></div><p className="mt-2 text-[11px] text-stone-600">Enter to send · Shift+Enter for a new line · Chat stays in this session</p></div>
  </main>
}

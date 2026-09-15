export interface ProviderError { code: string; message: string }
export type ModelProfileStatus = 'unknown' | 'experimental' | 'compatible' | 'limited'
export interface ModelProfile {
  label: string
  status: ModelProfileStatus
  chat: 'unknown' | 'supported' | 'limited'
  repositoryInspection: 'unknown' | 'supported' | 'limited'
  structuredEdits: 'unknown' | 'supported' | 'limited'
  resourceClass: string | null
}
export interface AIModel { id: string; name: string; profile: ModelProfile }
export interface ProviderStatus {
  state: 'connected' | 'offline' | 'error'
  models: AIModel[]
  error: ProviderError | null
}
export interface ChatMessage { role: 'user' | 'assistant'; content: string; model?: string; activity?: { label: string }[] }
export interface ChatRequest { model: string; messages: ChatMessage[] }
export interface PendingChange { path: string; before: string; after: string; replacements: number }
export interface PendingProposal { summary: string; changes: PendingChange[] }
export interface ChatResponse { model: string; content: string; activity: { label: string }[]; proposal: PendingProposal | null }
export interface AgentDebugStatus { enabled: boolean; hasTrace: boolean }

export interface ProviderError { code: string; message: string }
export interface AIModel { id: string; name: string }
export interface ProviderStatus {
  state: 'connected' | 'offline' | 'error'
  models: AIModel[]
  error: ProviderError | null
}
export interface ChatMessage { role: 'user' | 'assistant'; content: string; model?: string }
export interface ChatRequest { model: string; messages: ChatMessage[] }
export interface ChatResponse { model: string; content: string }

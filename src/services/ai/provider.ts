import type { ChatRequest, ChatResponse, ProviderStatus } from '../../types/ai'

export interface AIProvider {
  name: string
  getStatus(): Promise<ProviderStatus>
  chat(request: ChatRequest): Promise<ChatResponse>
}

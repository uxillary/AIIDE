import type { ChatRequest, ChatResponse, ProviderId, ProviderStatus } from '../../types/ai'

export interface AIProvider {
  id: ProviderId
  name: string
  getStatus(): Promise<ProviderStatus>
  chat(request: ChatRequest): Promise<ChatResponse>
}

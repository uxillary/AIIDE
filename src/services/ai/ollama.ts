import { invoke } from '@tauri-apps/api/core'
import type { ChatRequest, ChatResponse, ProviderStatus } from '../../types/ai'
import type { AIProvider } from './provider'

export const ollamaProvider: AIProvider = {
  name: 'Ollama',
  getStatus: () => invoke<ProviderStatus>('ollama_status'),
  chat: ({ model, messages }: ChatRequest) => invoke<ChatResponse>('ollama_chat', { model, messages }),
}

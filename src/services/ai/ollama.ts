import { invoke } from '@tauri-apps/api/core'
import type { AgentDebugStatus, ChatRequest, ChatResponse, ProviderId, ProviderStatus } from '../../types/ai'
import type { AIProvider } from './provider'

function createProvider(id: ProviderId, name: string): AIProvider {
  return {
    id,
    name,
    getStatus: () => invoke<ProviderStatus>('ollama_status', { providerId: id }),
    chat: ({ providerId, model, messages }: ChatRequest) => invoke<ChatResponse>('ollama_chat', { providerId, model, messages }),
  }
}

export const aiProviders: Record<ProviderId, AIProvider> = {
  ollama: createProvider('ollama', 'Ollama'),
  openrouter: createProvider('openrouter', 'OpenRouter'),
}

export const ollamaProvider = aiProviders.ollama

export const getAgentDebugStatus = () => invoke<AgentDebugStatus>('agent_debug_status')
export const setAgentDebug = (enabled: boolean) => invoke<AgentDebugStatus>('set_agent_debug', { enabled })
export const getLatestAgentTrace = () => invoke<string | null>('latest_agent_trace')

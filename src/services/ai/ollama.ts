import { invoke } from '@tauri-apps/api/core'
import type { AgentDebugStatus, ChatRequest, ChatResponse, ProviderStatus } from '../../types/ai'
import type { AIProvider } from './provider'

export const ollamaProvider: AIProvider = {
  name: 'Ollama',
  getStatus: () => invoke<ProviderStatus>('ollama_status'),
  chat: ({ model, messages }: ChatRequest) => invoke<ChatResponse>('ollama_chat', { model, messages }),
}

export const getAgentDebugStatus = () => invoke<AgentDebugStatus>('agent_debug_status')
export const setAgentDebug = (enabled: boolean) => invoke<AgentDebugStatus>('set_agent_debug', { enabled })
export const getLatestAgentTrace = () => invoke<string | null>('latest_agent_trace')

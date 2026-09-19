export type ImageJobStatus = 'submitting' | 'queued' | 'generating' | 'ready' | 'failed' | 'cancelled'

export interface ImageJob {
  jobId: string
  prompt: string
  seed: number
  status: ImageJobStatus
  statusLabel: string
  previewAvailable: boolean
  cancellationSupported: boolean
  error: string | null
}

export interface ImageEngineStatus {
  state: 'connected' | 'offline' | 'error'
  endpoint: string
  checkpoint: string
  busy: boolean
  error: string | null
}

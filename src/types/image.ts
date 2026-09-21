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

export interface ImageModelSummary {
  id: string
  displayName: string
  architecture: string
  capabilities: string[]
  engineRequirement: string
  workflowId: string
  checkpoint: string
  supportingFiles: string[]
  requiredNodes: string[]
  hardwareGuidance: string
  license: string
  acquisition: string
}

export interface ImageEngineStatus {
  state: 'unavailable' | 'incompatible' | 'missing_nodes' | 'missing_checkpoint' | 'ready' | 'busy'
  ready: boolean
  endpoint: string
  model: ImageModelSummary
  checkpoint: string
  busy: boolean
  engineStatus: 'unavailable' | 'reachable' | 'incompatible' | 'busy'
  modelStatus: 'unknown' | 'missing_nodes' | 'missing_checkpoint' | 'ready'
  hardwareStatus: 'unavailable' | 'potentially_insufficient' | 'sufficient'
  missingNodes: string[]
  missingFiles: string[]
  hardwareMessage: string | null
  error: string | null
}

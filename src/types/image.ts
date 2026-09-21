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
  state: 'unavailable' | 'incompatible' | 'missing_nodes' | 'missing_checkpoint' | 'managed_not_installed' | 'managed_installed' | 'starting' | 'ready' | 'busy' | 'failed'
  ready: boolean
  ownershipMode: 'external' | 'managed'
  managedState: 'disabled' | 'not_installed' | 'installed' | 'starting' | 'ready' | 'failed' | 'incompatible'
  managedAcquisitionEnabled: boolean
  managedMessage: string | null
  endpoint: string
  model: ImageModelSummary
  checkpoint: string
  busy: boolean
  engineStatus: 'unavailable' | 'reachable' | 'incompatible' | 'starting' | 'busy'
  modelStatus: 'unknown' | 'missing_nodes' | 'missing_checkpoint' | 'ready'
  hardwareStatus: 'unavailable' | 'potentially_insufficient' | 'sufficient'
  missingNodes: string[]
  missingFiles: string[]
  hardwareMessage: string | null
  error: string | null
}

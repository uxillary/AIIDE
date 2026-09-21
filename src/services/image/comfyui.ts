import { invoke } from '@tauri-apps/api/core'
import type { ImageProvider } from './provider'
import type { ImageEngineStatus, ImageJob } from '../../types/image'

export const comfyUiProvider: ImageProvider = {
  name: 'ComfyUI',
  getStatus: () => invoke<ImageEngineStatus>('image_generation_status'),
  configure: (endpoint: string, modelId: string) => invoke<ImageEngineStatus>('configure_image_generation', { endpoint, modelId }),
  start: (prompt: string) => invoke<ImageJob>('start_image_generation', { prompt }),
  getJob: (jobId: string) => invoke<ImageJob>('get_image_generation', { jobId }),
  getPreview: (jobId: string) => invoke<ArrayBuffer>('get_image_preview', { jobId }),
  cancel: (jobId: string) => invoke<ImageJob>('cancel_image_generation', { jobId }),
  reject: (jobId: string) => invoke<void>('reject_generated_image', { jobId }),
  save: (jobId: string, relativePath: string) => invoke<string>('save_generated_image', { jobId, relativePath }),
}

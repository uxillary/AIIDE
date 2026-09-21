import type { ImageEngineStatus, ImageJob } from '../../types/image'

export interface ImageProvider {
  name: string
  getStatus(): Promise<ImageEngineStatus>
  configure(endpoint: string, modelId: string): Promise<ImageEngineStatus>
  start(prompt: string): Promise<ImageJob>
  getJob(jobId: string): Promise<ImageJob>
  getPreview(jobId: string): Promise<ArrayBuffer>
  cancel(jobId: string): Promise<ImageJob>
  reject(jobId: string): Promise<void>
  save(jobId: string, relativePath: string): Promise<string>
}

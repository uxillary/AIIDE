import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { ProjectInfo } from '../types/project'

export async function chooseProject(): Promise<ProjectInfo | null> {
  const path = await open({ directory: true, multiple: false, title: 'Open project folder' })
  if (!path) return null
  return invoke<ProjectInfo>('inspect_project', { path })
}

export const refreshProject = (path: string) => invoke<ProjectInfo>('inspect_project', { path })
export const applyPendingChange = () => invoke<void>('apply_pending_change')
export const rejectPendingChange = () => invoke<void>('reject_pending_change')
export const viewRepositoryFile = (path: string) => invoke<{ content: string; truncated: boolean }>('view_repository_file', { path })

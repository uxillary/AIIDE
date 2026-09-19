import { invoke } from '@tauri-apps/api/core'
import type { CommitDetail, CommitPreview, CommitResult, GitCommit, GitDiff, GitStatus } from '../types/git'

export const getGitStatus = () => invoke<GitStatus>('git_status')
export const getGitDiff = (path: string, staged: boolean) => invoke<GitDiff>('git_file_diff', { path, staged })
export const stageGitFile = (path: string) => invoke<GitStatus>('git_stage_file', { path })
export const unstageGitFile = (path: string) => invoke<GitStatus>('git_unstage_file', { path })
export const prepareGitCommit = () => invoke<CommitPreview>('git_commit_preview')
export const createGitCommit = (message: string, token: string) => invoke<CommitResult>('git_commit', { message, token, approved: true })
export const suggestGitCommitMessage = (model: string) => invoke<string>('git_suggest_commit_message', { model })
export const getGitHistory = () => invoke<GitCommit[]>('git_history')
export const getGitCommitDetail = (hash: string) => invoke<CommitDetail>('git_commit_detail', { hash })

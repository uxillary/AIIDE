export interface GitFileStatus {
  path: string
  originalPath: string | null
  kind: 'modified' | 'added' | 'deleted' | 'renamed' | 'untracked'
  staged: boolean
  unstaged: boolean
  conflict: boolean
}

export interface GitStatus {
  branch: string
  detached: boolean
  files: GitFileStatus[]
  clean: boolean
  hasConflicts: boolean
}

export interface GitDiff { path: string; content: string; truncated: boolean; binary: boolean; untracked: boolean }
export interface CommitPreview { branch: string; files: string[]; summary: string; token: string }
export interface CommitResult { hash: string; subject: string }
export interface GitCommit { hash: string; subject: string; date: string }
export interface CommitDetail { hash: string; summary: string }

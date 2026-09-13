export interface TreeEntry {
  name: string
  relativePath: string
  kind: 'file' | 'directory'
  children?: TreeEntry[]
  truncated?: boolean
}

export interface RepositoryInfo {
  name: string
  branch: string
  changedFiles: number
}

export interface ProjectInfo {
  name: string
  path: string
  repository: RepositoryInfo | null
  tree: TreeEntry[]
  treeTruncated: boolean
}

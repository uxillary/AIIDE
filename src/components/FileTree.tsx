import { useState } from 'react'
import type { TreeEntry } from '../types/project'

type IconKind = 'folder' | 'file' | 'html' | 'css' | 'js' | 'ts' | 'json' | 'markdown' | 'image' | 'config'

function fileIconKind(entry: TreeEntry): IconKind {
  if (entry.kind === 'directory') return 'folder'
  const name = entry.name.toLowerCase()
  if (/^(\.gitignore|\.gitattributes|\.editorconfig|\.env(?:\..*)?|.*\.config\.[cm]?[jt]s|.*\.ya?ml|.*\.toml|.*\.ini)$/.test(name)) return 'config'
  const extension = name.slice(name.lastIndexOf('.'))
  if (['.html', '.htm'].includes(extension)) return 'html'
  if (['.css', '.scss', '.sass', '.less'].includes(extension)) return 'css'
  if (['.js', '.jsx', '.mjs', '.cjs'].includes(extension)) return 'js'
  if (['.ts', '.tsx'].includes(extension)) return 'ts'
  if (extension === '.json') return 'json'
  if (['.md', '.mdx'].includes(extension)) return 'markdown'
  if (['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg', '.ico'].includes(extension)) return 'image'
  return 'file'
}

export function FileIcon({ kind }: { kind: IconKind }) {
  if (kind === 'folder') return <svg className="file-type-icon file-type-folder" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 4.5v-2h5l2 2h6v9h-13z" stroke="currentColor" strokeWidth="1" strokeLinejoin="round" /></svg>
  if (kind === 'image') return <svg className="file-type-icon file-type-image" viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="1.5" y="1.5" width="13" height="13" stroke="currentColor"/><circle cx="5" cy="5" r="1" fill="currentColor"/><path d="m2.5 12.5 4-4 2 2 2-2 3 3" stroke="currentColor" strokeLinejoin="round"/></svg>
  if (kind === 'config') return <svg className="file-type-icon file-type-config" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 4.5h13M1.5 8.5h13M1.5 12.5h13" stroke="currentColor"/><path d="M5.5 2.5v4M10.5 6.5v4M5.5 10.5v4" stroke="currentColor" strokeWidth="2"/></svg>
  const label: Partial<Record<IconKind, string>> = { html: '<>', css: '#', js: 'JS', ts: 'TS', json: '{}', markdown: 'M' }
  return <svg className={`file-type-icon file-type-${kind}`} viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3.5 1.5h6l3 3v10h-9z" stroke="currentColor" strokeLinejoin="round"/><path d="M9.5 1.5v3h3" stroke="currentColor"/>{label[kind] && <text x="8" y="11.5" textAnchor="middle" fill="currentColor" fontSize="6" fontFamily="monospace" fontWeight="bold">{label[kind]}</text>}</svg>
}

function TreeNode({ entry, depth }: { entry: TreeEntry; depth: number }) {
  const [expanded, setExpanded] = useState(depth < 1)
  const directory = entry.kind === 'directory'
  return <li>
    <button type="button" onClick={() => directory && setExpanded(!expanded)} className="tree-row" style={{ paddingLeft: `${12 + depth * 15}px` }} aria-expanded={directory ? expanded : undefined}>
      <span className="tree-chevron" aria-hidden="true">{directory ? (expanded ? '▾' : '▸') : ''}</span>
      <FileIcon kind={fileIconKind(entry)} />
      <span className={`tree-name ${directory ? 'tree-name-folder' : ''}`}>{entry.name}</span>
    </button>
    {directory && expanded && <ul>{entry.children?.map(child => <TreeNode key={child.relativePath} entry={child} depth={depth + 1} />)}{entry.truncated && <li className="tree-hint" style={{ paddingLeft: `${27 + depth * 15}px` }}>Further contents not loaded</li>}</ul>}
  </li>
}

export function FileTree({ entries, truncated }: { entries: TreeEntry[]; truncated: boolean }) {
  if (!entries.length) return <p className="px-4 py-3 text-xs text-stone-500">No visible files in this folder.</p>
  return <div className="file-tree-scroll"><ul>{entries.map(entry => <TreeNode key={entry.relativePath} entry={entry} depth={0} />)}</ul>{truncated && <p className="tree-hint px-4 py-2">Tree limit reached. Some files are not shown.</p>}</div>
}

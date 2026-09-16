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
  if (kind === 'folder') return <svg className="file-type-icon file-type-folder" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 4.5V3.7c0-.7.5-1.2 1.2-1.2h3.1l1.5 1.7h6c.7 0 1.2.5 1.2 1.2v7.1c0 .7-.5 1.2-1.2 1.2H2.7c-.7 0-1.2-.5-1.2-1.2v-8Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" /></svg>
  if (kind === 'image') return <svg className="file-type-icon file-type-image" viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="1.5" stroke="currentColor" strokeWidth="1.2"/><circle cx="5.5" cy="5.5" r="1" fill="currentColor"/><path d="m3 12 3.3-3.3 2 2L10 9l3 3" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>
  if (kind === 'config') return <svg className="file-type-icon file-type-config" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2.5 4h11M2.5 8h11M2.5 12h11" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/><circle cx="6" cy="4" r="1.3" fill="#151816" stroke="currentColor" strokeWidth="1.2"/><circle cx="10" cy="8" r="1.3" fill="#151816" stroke="currentColor" strokeWidth="1.2"/><circle cx="6" cy="12" r="1.3" fill="#151816" stroke="currentColor" strokeWidth="1.2"/></svg>
  const label: Partial<Record<IconKind, string>> = { html: '<>', css: '#', js: 'JS', ts: 'TS', json: '{}', markdown: 'M' }
  return <svg className={`file-type-icon file-type-${kind}`} viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3 1.5h6.5l3.5 3.5v9.2c0 .7-.5 1.3-1.2 1.3H4.2c-.7 0-1.2-.6-1.2-1.3V2.8c0-.7.5-1.3 1.2-1.3Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/><path d="M9.5 1.7V5H13" stroke="currentColor" strokeWidth="1.1"/>{label[kind] && <text x="8" y="11.7" textAnchor="middle" fill="currentColor" fontSize="5.5" fontFamily="monospace" fontWeight="bold">{label[kind]}</text>}</svg>
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

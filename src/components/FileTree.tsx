import { useState } from 'react'
import type { TreeEntry } from '../types/project'

function TreeNode({ entry, depth }: { entry: TreeEntry; depth: number }) {
  const [expanded, setExpanded] = useState(depth < 1)
  const directory = entry.kind === 'directory'
  return <li>
    <button type="button" onClick={() => directory && setExpanded(!expanded)} className="tree-row" style={{ paddingLeft: `${12 + depth * 15}px` }} aria-expanded={directory ? expanded : undefined}>
      <span className="tree-icon" aria-hidden="true">{directory ? (expanded ? '▾' : '▸') : '·'}</span>
      <span className={directory ? 'text-stone-200' : 'text-stone-400'}>{entry.name}</span>
    </button>
    {directory && expanded && <ul>{entry.children?.map(child => <TreeNode key={child.relativePath} entry={child} depth={depth + 1} />)}{entry.truncated && <li className="tree-hint" style={{ paddingLeft: `${27 + depth * 15}px` }}>Further contents not loaded</li>}</ul>}
  </li>
}

export function FileTree({ entries, truncated }: { entries: TreeEntry[]; truncated: boolean }) {
  if (!entries.length) return <p className="px-4 py-3 text-xs text-stone-500">No visible files in this folder.</p>
  return <div className="overflow-y-auto"><ul>{entries.map(entry => <TreeNode key={entry.relativePath} entry={entry} depth={0} />)}</ul>{truncated && <p className="tree-hint px-4 py-2">Tree limit reached. Some files are not shown.</p>}</div>
}

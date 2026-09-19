import type { Region } from './geometry'
import type { GridOptions } from './detection'

const GRID_FIELDS: { key: keyof GridOptions; label: string }[] = [
  { key: 'rows', label: 'Rows' }, { key: 'columns', label: 'Columns' },
  { key: 'gapX', label: 'X gap' }, { key: 'gapY', label: 'Y gap' },
  { key: 'left', label: 'Left' }, { key: 'right', label: 'Right' },
  { key: 'top', label: 'Top' }, { key: 'bottom', label: 'Bottom' },
]

type Props = {
  enabled: boolean; mode: 'grid' | 'alpha'; onMode: (mode: 'grid' | 'alpha') => void
  grid: GridOptions; onGrid: (grid: GridOptions) => void
  joinGap: number; onJoinGap: (value: number) => void; minPixels: number; onMinPixels: (value: number) => void
  suggestions: Region[]; selected: Region | null; eligibleCount: number; onSelected: (id: string) => void
  onGenerate: () => void; onEdit: (key: 'x' | 'y' | 'width' | 'height', value: string) => void
  onAccept: (id: string) => void; onAcceptAll: () => void; onReject: (id: string) => void; onClear: () => void
}

export function DetectionPanel(props: Props) {
  const { enabled, mode, onMode, grid, onGrid, joinGap, onJoinGap, minPixels, onMinPixels, suggestions, selected, eligibleCount, onSelected, onGenerate, onEdit, onAccept, onAcceptAll, onReject, onClear } = props
  return <details className="detector"><summary>✦ Frame suggestions <span>{suggestions.length || ''}</span></summary><div className="detector-body">
    <p>Conservative geometry only (up to 256 grid cells). Labels and detached effects may need manual correction.</p>
    <label className="field"><span>Detection method</span><select value={mode} onChange={event => onMode(event.target.value as 'grid' | 'alpha')}><option value="grid">Regular grid</option><option value="alpha">Alpha islands</option></select></label>
    {mode === 'grid' ? <div className="detector-grid">{GRID_FIELDS.map(({ key, label }) => <label className="field" key={key}><span>{label}</span><input type="number" min={key === 'rows' || key === 'columns' ? 1 : 0} max={key === 'rows' || key === 'columns' ? 64 : 16384} value={grid[key]} onChange={event => onGrid({ ...grid, [key]: Number(event.target.value) || 0 })} /></label>)}</div> : <div className="detector-grid"><label className="field"><span>Join gap</span><input type="number" min="0" max="256" value={joinGap} onChange={event => onJoinGap(Number(event.target.value) || 0)} /></label><label className="field"><span>Min pixels</span><input type="number" min="1" max="10000" value={minPixels} onChange={event => onMinPixels(Number(event.target.value) || 1)} /></label></div>}
    <button className="secondary detector-generate" disabled={!enabled} onClick={onGenerate}>Preview suggestions</button>
    {suggestions.length > 0 && <><button className="primary accept-all" disabled={eligibleCount === 0} onClick={onAcceptAll}>Add all suggested frames{eligibleCount > 0 ? ` (${eligibleCount})` : ''}</button><div className="suggestion-head"><strong>{suggestions.length} pending</strong><button onClick={onClear}>Reject all</button></div><div className="suggestion-list">{suggestions.map(item => <button key={item.id} className={selected?.id === item.id ? 'suggestion active' : 'suggestion'} onClick={() => onSelected(item.id)}><span>{item.name}</span><small>{item.x}, {item.y} · {item.width} × {item.height}</small></button>)}</div>{selected && <><div className="detector-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>{key}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} value={selected[key]} onChange={event => onEdit(key, event.target.value)} /></label>)}</div><div className="suggestion-actions"><button className="primary" onClick={() => onAccept(selected.id)}>Accept region</button><button className="danger" onClick={() => onReject(selected.id)}>Reject</button></div></>}</>}
  </div></details>
}

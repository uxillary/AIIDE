import type { GridOptions, RowBoundaryMode, RowSuggestion } from './detection'
import type { Region } from './geometry'
import { Thumbnail } from './Preview'

const GRID_FIELDS: { key: keyof GridOptions; label: string }[] = [
  { key: 'rows', label: 'Rows' }, { key: 'columns', label: 'Columns' }, { key: 'gapX', label: 'X gap' }, { key: 'gapY', label: 'Y gap' },
  { key: 'left', label: 'Left' }, { key: 'right', label: 'Right' }, { key: 'top', label: 'Top' }, { key: 'bottom', label: 'Bottom' },
]

type Props = {
  enabled: boolean; image: ImageBitmap | null; mode: 'grid' | 'alpha' | 'row'; onMode: (mode: 'grid' | 'alpha' | 'row') => void
  grid: GridOptions; onGrid: (grid: GridOptions) => void
  joinGap: number; onJoinGap: (value: number) => void; minPixels: number; onMinPixels: (value: number) => void
  rowSelection: Region | null; onRowEdit: (key: 'x' | 'y' | 'width' | 'height', value: string) => void
  rowFrameCount: number; onRowFrameCount: (value: number) => void; rowBoundaryMode: RowBoundaryMode; onRowBoundaryMode: (value: RowBoundaryMode) => void
  rowPadding: number; onRowPadding: (value: number) => void; assignBatch: boolean; onAssignBatch: (value: boolean) => void
  suggestions: RowSuggestion[]; selected: RowSuggestion | null; eligibleCount: number; onSelected: (id: string) => void
  onGenerate: () => void; onEdit: (key: 'x' | 'y' | 'width' | 'height', value: string) => void
  onAccept: (id: string) => void; onAcceptAll: () => void; onReject: (id: string) => void; onClear: () => void
}

export function DetectionPanel(props: Props) {
  const { enabled, image, mode, onMode, grid, onGrid, joinGap, onJoinGap, minPixels, onMinPixels, rowSelection, onRowEdit, rowFrameCount, onRowFrameCount, rowBoundaryMode, onRowBoundaryMode, rowPadding, onRowPadding, assignBatch, onAssignBatch, suggestions, selected, eligibleCount, onSelected, onGenerate, onEdit, onAccept, onAcceptAll, onReject, onClear } = props
  const reviewingRow = mode === 'row' && suggestions.length > 0
  return <details className="detector" open><summary>✦ Frame suggestions <span>{suggestions.length || ''}</span></summary><div className="detector-body">
    {mode === 'row' && <ol className="row-steps" aria-label="Animation row workflow"><li className={!rowSelection ? 'active' : ''}>Select row</li><li className={rowSelection && !suggestions.length ? 'active' : ''}>Detect frames</li><li className={reviewingRow ? 'active' : ''}>Review</li><li>Add frames</li></ol>}
    <p>Geometry only. Labels, numbers, backgrounds and detached effects are not interpreted semantically.</p>
    <label className="field"><span>Detection method</span><select value={mode} onChange={event => onMode(event.target.value as Props['mode'])}><option value="grid">Regular grid</option><option value="alpha">Alpha islands</option><option value="row">Animation row</option></select></label>
    {mode === 'grid' && <div className="detector-grid">{GRID_FIELDS.map(({ key, label }) => <label className="field" key={key}><span>{label}</span><input type="number" min={key === 'rows' || key === 'columns' ? 1 : 0} max={key === 'rows' || key === 'columns' ? 64 : 16384} value={grid[key]} onChange={event => onGrid({ ...grid, [key]: Number(event.target.value) || 0 })} /></label>)}</div>}
    {mode === 'alpha' && <div className="detector-grid"><label className="field"><span>Join gap</span><input type="number" min="0" max="256" value={joinGap} onChange={event => onJoinGap(Number(event.target.value) || 0)} /></label><label className="field"><span>Min pixels</span><input type="number" min="1" max="10000" value={minPixels} onChange={event => onMinPixels(Number(event.target.value) || 1)} /></label></div>}
    {mode === 'row' && <>
      <p className="row-instruction">{rowSelection ? 'Adjust the green row box on the image or enter exact original-pixel coordinates.' : 'Drag across one animation row on the image.'}</p>
      {rowSelection && <div className="detector-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>Row {key}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} value={rowSelection[key]} onChange={event => onRowEdit(key, event.target.value)} /></label>)}</div>}
      <div className="detector-grid"><label className="field"><span>Expected frames</span><input type="number" min="1" max="64" value={rowFrameCount} onChange={event => onRowFrameCount(Math.max(1, Math.min(64, Number(event.target.value) || 1)))} /></label><label className="field"><span>Boundaries</span><select value={rowBoundaryMode} onChange={event => onRowBoundaryMode(event.target.value as RowBoundaryMode)}><option value="equal">Equal spacing</option><option value="content">Content-assisted</option></select></label></div>
      <details className="advanced"><summary>Advanced settings</summary><label className="field"><span>Boundary padding (px)</span><input type="number" min="0" max="256" value={rowPadding} onChange={event => onRowPadding(Math.max(0, Math.min(256, Number(event.target.value) || 0)))} /></label></details>
    </>}
    <button className="secondary detector-generate" disabled={!enabled || (mode === 'row' && !rowSelection)} onClick={onGenerate}>{reviewingRow ? 'Detect again' : 'Preview suggestions'}</button>
    {suggestions.length > 0 && <>
      {mode === 'row' && <label className="check-field assign-batch"><input type="checkbox" checked={assignBatch} onChange={event => onAssignBatch(event.target.checked)} /> Assign accepted batch to animation slots</label>}
      <button className="primary accept-all" disabled={eligibleCount === 0} onClick={onAcceptAll}>{mode === 'row' ? `Add ${suggestions.length} frames` : `Add all suggested frames${eligibleCount > 0 ? ` (${eligibleCount})` : ''}`}</button>
      <div className="suggestion-head"><strong>{suggestions.length} pending</strong><button onClick={onClear}>{mode === 'row' ? 'Back to row selection' : 'Reject all'}</button></div>
      <div className="suggestion-list">{suggestions.map((item, index) => <button key={item.id} className={selected?.id === item.id ? 'suggestion active' : 'suggestion'} onClick={() => onSelected(item.id)}>{image && <Thumbnail image={image} region={item} />}<span><strong>{index + 1}. {item.name}</strong><small>{item.x}, {item.y} · {item.width} × {item.height}</small>{item.needsReview && <em>⚠ {item.reviewReason}</em>}</span></button>)}</div>
      {selected && <><div className="detector-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>{key}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} value={selected[key]} onChange={event => onEdit(key, event.target.value)} /></label>)}</div><div className="suggestion-actions"><button className="primary" onClick={() => onAccept(selected.id)}>Accept region</button><button className="danger" onClick={() => onReject(selected.id)}>Reject</button></div></>}
    </>}
  </div></details>
}

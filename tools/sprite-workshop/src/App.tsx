import { useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties, ReactNode } from 'react'
import { boundedRegion, clamp, fitZoom, regionFromCorners, resizeRegion, screenToImage, zoomAt } from './geometry'
import type { Handle, Point, Region } from './geometry'
import { downloadAlignedSlot, downloadRegion } from './export'
import { loadPng } from './image'
import type { SourceImage } from './image'
import { SpriteCanvas, Thumbnail } from './Preview'
import { alignmentDragOffset, alignmentScale, calculateLayout, emptyOffsets, referenceSlotFor } from './alignment'
import type { AlignmentMode, AlignmentZoom, Pixels } from './alignment'
import { acceptSuggestion, alphaSuggestions, gridSuggestions, rejectSuggestion } from './detection'
import type { GridOptions } from './detection'
import { removeFrame, restoreFrame } from './frameActions'
import type { DeletedFrame } from './frameActions'
import { DetectionPanel } from './DetectionPanel'

type Drag =
  | { kind: 'draw'; start: Point; id: string; name: string }
  | { kind: 'pan'; start: Point; pan: Point }
  | { kind: 'move' | 'resize'; start: Point; region: Region; handle?: Handle }

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']
const EMPTY_SLOTS = Array<string | null>(8).fill(null)
const PANEL_KEYS = ['frames', 'inspection', 'alignment', 'offsets', 'preview', 'slots'] as const
type PanelKey = typeof PANEL_KEYS[number]
type PanelVisibility = Record<PanelKey, boolean>
type WorkshopLayout = { panels: PanelVisibility; leftWidth: number; rightWidth: number }
const DEFAULT_LAYOUT: WorkshopLayout = {
  panels: { frames: true, inspection: true, alignment: true, offsets: true, preview: true, slots: true },
  leftWidth: 255,
  rightWidth: 320,
}
const LAYOUT_STORAGE_KEY = 'sprite-workshop-layout-v1'

function readLayout(): WorkshopLayout {
  try {
    const saved = JSON.parse(window.localStorage.getItem(LAYOUT_STORAGE_KEY) ?? 'null') as Partial<WorkshopLayout> | null
    if (!saved || typeof saved !== 'object') return DEFAULT_LAYOUT
    const panels = Object.fromEntries(PANEL_KEYS.map(key => [key, typeof saved.panels?.[key] === 'boolean' ? saved.panels[key] : true])) as PanelVisibility
    return {
      panels,
      leftWidth: typeof saved.leftWidth === 'number' && Number.isFinite(saved.leftWidth) ? clamp(saved.leftWidth, 200, 360) : DEFAULT_LAYOUT.leftWidth,
      rightWidth: typeof saved.rightWidth === 'number' && Number.isFinite(saved.rightWidth) ? clamp(saved.rightWidth, 250, 420) : DEFAULT_LAYOUT.rightWidth,
    }
  } catch { return DEFAULT_LAYOUT }
}

function PanelHeader({ id, title, eyebrow, expanded, onToggle, extra }: { id: PanelKey; title: string; eyebrow?: string; expanded: boolean; onToggle: () => void; extra?: ReactNode }) {
  return <div className="collapsible-head">
    <button className="panel-toggle" aria-expanded={expanded} aria-controls={`panel-${id}`} onClick={onToggle}>
      <span className="panel-chevron" aria-hidden="true">⌄</span><span>{eyebrow && <span className="eyebrow">{eyebrow}</span>}<strong>{title}</strong></span>
    </button>{extra}
  </div>
}

export default function App() {
  const [workshopLayout, setWorkshopLayout] = useState(readLayout)
  const [viewportWidth, setViewportWidth] = useState(() => window.innerWidth)
  const [source, setSource] = useState<SourceImage | null>(null)
  const sourceRef = useRef<SourceImage | null>(null)
  const [regions, setRegions] = useState<Region[]>([])
  const [pixels, setPixels] = useState<Pixels | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [deleted, setDeleted] = useState<DeletedFrame | null>(null)
  const [suggestions, setSuggestions] = useState<Region[]>([])
  const [selectedSuggestion, setSelectedSuggestion] = useState<string | null>(null)
  const [detectionMode, setDetectionMode] = useState<'grid' | 'alpha'>('grid')
  const [grid, setGrid] = useState<GridOptions>({ rows: 5, columns: 8, gapX: 8, gapY: 8, left: 0, right: 0, top: 0, bottom: 0 })
  const [joinGap, setJoinGap] = useState(12)
  const [minPixels, setMinPixels] = useState(8)
  const [slots, setSlots] = useState<(string | null)[]>(EMPTY_SLOTS)
  const [activeSlot, setActiveSlot] = useState(0)
  const [fps, setFps] = useState(8)
  const [playing, setPlaying] = useState(false)
  const [reducedMotion, setReducedMotion] = useState(false)
  const [alignmentMode, setAlignmentMode] = useState<AlignmentMode>('bottom')
  const [offsets, setOffsets] = useState<Point[]>(emptyOffsets)
  const [onion, setOnion] = useState(false)
  const [onionReference, setOnionReference] = useState<'previous' | 'fixed'>('previous')
  const [fixedReferenceSlot, setFixedReferenceSlot] = useState(0)
  const [padding, setPadding] = useState(8)
  const [minWidth, setMinWidth] = useState(0)
  const [minHeight, setMinHeight] = useState(0)
  const [centreGuide, setCentreGuide] = useState(true)
  const [baselineGuide, setBaselineGuide] = useState(true)
  const [pixelGrid, setPixelGrid] = useState(false)
  const [baselineOffset, setBaselineOffset] = useState(0)
  const [alignmentZoom, setAlignmentZoom] = useState<AlignmentZoom>('fit')
  const [alignmentPan, setAlignmentPan] = useState<Point>({ x: 0, y: 0 })
  const [alignmentPanMode, setAlignmentPanMode] = useState(false)
  const [alignmentExpanded, setAlignmentExpanded] = useState(false)
  const [alignmentViewport, setAlignmentViewport] = useState({ width: 288, height: 300 })
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<Point>({ x: 0, y: 0 })
  const [mode, setMode] = useState<'select' | 'pan'>('select')
  const [spaceHeld, setSpaceHeld] = useState(false)
  const [draft, setDraft] = useState<Region | null>(null)
  const [dragOver, setDragOver] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const stageRef = useRef<HTMLDivElement>(null)
  const alignmentPreviewRef = useRef<HTMLDivElement>(null)
  const sourceCanvasRef = useRef<HTMLCanvasElement>(null)
  const dragRef = useRef<Drag | null>(null)
  const alignmentDrag = useRef<{ kind: 'move' | 'pan'; x: number; y: number; offset: Point; slot: number; scale: number } | null>(null)
  const nextNumber = useRef(1)
  const loadCounter = useRef(0)
  const selected = regions.find(region => region.id === selectedId) ?? null
  const suggestion = suggestions.find(region => region.id === selectedSuggestion) ?? null
  const imageSize = source ? { width: source.width, height: source.height } : null
  const assigned = useMemo(() => slots.map(id => regions.find(region => region.id === id) ?? null), [slots, regions])
  const layout = useMemo(() => calculateLayout(assigned, offsets, { mode: alignmentMode, padding, minWidth, minHeight }), [assigned, offsets, alignmentMode, padding, minWidth, minHeight])
  const referenceSlot = referenceSlotFor(activeSlot, onionReference, fixedReferenceSlot)
  const previewScale = alignmentScale(alignmentZoom, layout, alignmentViewport)
  const baselineY = clamp(layout.anchorY + baselineOffset, 0, layout.height - 1)
  const leftWidth = Math.min(workshopLayout.leftWidth, Math.max(200, viewportWidth - workshopLayout.rightWidth - 332))
  const rightWidth = Math.min(workshopLayout.rightWidth, Math.max(250, viewportWidth - leftWidth - 332))
  const workspaceStyle = { '--left-width': `${leftWidth}px`, '--right-width': `${rightWidth}px` } as CSSProperties

  function togglePanel(key: PanelKey) {
    setWorkshopLayout(previous => ({ ...previous, panels: { ...previous.panels, [key]: !previous.panels[key] } }))
  }

  function resizeSidebar(side: 'left' | 'right', width: number) {
    const minimum = side === 'left' ? 200 : 250
    const maximum = side === 'left' ? 360 : 420
    const other = side === 'left' ? rightWidth : leftWidth
    setWorkshopLayout(previous => ({ ...previous, [side === 'left' ? 'leftWidth' : 'rightWidth']: clamp(Math.round(width), minimum, Math.max(minimum, Math.min(maximum, window.innerWidth - other - 332))) }))
  }

  function resizeHandle(side: 'left' | 'right') {
    const width = side === 'left' ? leftWidth : rightWidth
    return <div className="sidebar-resizer" role="separator" tabIndex={0} aria-label={`Resize ${side} sidebar`} aria-orientation="vertical" aria-valuemin={side === 'left' ? 200 : 250} aria-valuemax={side === 'left' ? 360 : 420} aria-valuenow={width}
      onPointerDown={event => { event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.dataset.startX = String(event.clientX); event.currentTarget.dataset.startWidth = String(width) }}
      onPointerMove={event => { if (!event.currentTarget.hasPointerCapture(event.pointerId)) return; const delta = event.clientX - Number(event.currentTarget.dataset.startX); resizeSidebar(side, Number(event.currentTarget.dataset.startWidth) + (side === 'left' ? delta : -delta)) }}
      onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}
      onKeyDown={event => { const direction = event.key === 'ArrowLeft' ? -1 : event.key === 'ArrowRight' ? 1 : 0; if (!direction && event.key !== 'Home' && event.key !== 'End') return; event.preventDefault(); resizeSidebar(side, event.key === 'Home' ? 0 : event.key === 'End' ? 1000 : width + direction * (event.shiftKey ? 25 : 10) * (side === 'left' ? 1 : -1)) }} />
  }

  useEffect(() => {
    try { window.localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(workshopLayout)) } catch { /* Layout remains usable without storage. */ }
  }, [workshopLayout])
  useEffect(() => {
    const update = () => setViewportWidth(window.innerWidth)
    window.addEventListener('resize', update)
    return () => window.removeEventListener('resize', update)
  }, [])

  useEffect(() => {
    const preview = alignmentPreviewRef.current
    if (!preview) return
    const observer = new ResizeObserver(() => setAlignmentViewport({ width: preview.clientWidth, height: preview.clientHeight }))
    observer.observe(preview)
    return () => observer.disconnect()
  }, [alignmentExpanded])

  useEffect(() => () => { sourceRef.current?.bitmap.close() }, [])
  useEffect(() => {
    const query = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => { setReducedMotion(query.matches); if (query.matches) setPlaying(false) }
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  }, [])
  useEffect(() => {
    const down = (event: KeyboardEvent) => { if (event.code === 'Space' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLSelectElement) && !(event.target instanceof HTMLButtonElement)) { event.preventDefault(); setSpaceHeld(true) } }
    const up = (event: KeyboardEvent) => { if (event.code === 'Space') setSpaceHeld(false) }
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    return () => { window.removeEventListener('keydown', down); window.removeEventListener('keyup', up) }
  }, [])
  useEffect(() => {
    const canvas = sourceCanvasRef.current
    if (!canvas || !source) return
    const context = canvas.getContext('2d')
    if (!context) { setError('Canvas is unavailable in this browser.'); return }
    context.clearRect(0, 0, source.width, source.height)
    context.imageSmoothingEnabled = false
    context.drawImage(source.bitmap, 0, 0)
  }, [source])
  useEffect(() => {
    if (!playing || reducedMotion || !assigned.some(Boolean)) return
    const timer = window.setInterval(() => setActiveSlot(index => (index + 1) % 8), 1000 / fps)
    return () => window.clearInterval(timer)
  }, [playing, reducedMotion, fps, assigned])

  function fit(image: SourceImage = source!) {
    const stage = stageRef.current
    if (!stage || !image) return
    const next = fitZoom(image, { width: stage.clientWidth, height: stage.clientHeight })
    setZoom(next)
    setPan({ x: (stage.clientWidth - image.width * next) / 2, y: (stage.clientHeight - image.height * next) / 2 })
  }

  async function importFile(file?: File) {
    if (!file) return
    const request = ++loadCounter.current
    setError(null); setNotice(null)
    try {
      const loaded = await loadPng(file)
      if (request !== loadCounter.current) { loaded.bitmap.close(); return }
      const canvas = document.createElement('canvas')
      canvas.width = loaded.width; canvas.height = loaded.height
      const context = canvas.getContext('2d', { willReadFrequently: true })
      if (!context) { loaded.bitmap.close(); throw new Error('Canvas is unavailable for alpha analysis.') }
      context.drawImage(loaded.bitmap, 0, 0)
      let imageData: ImageData
      try { imageData = context.getImageData(0, 0, loaded.width, loaded.height) }
      catch { loaded.bitmap.close(); throw new Error('Could not read PNG pixels for local alpha analysis.') }
      sourceRef.current?.bitmap.close()
      sourceRef.current = loaded
      setSource(loaded); setPixels({ data: imageData.data, width: loaded.width, height: loaded.height }); setRegions([]); setSelectedId(null); setSlots([...EMPTY_SLOTS]); setOffsets(emptyOffsets()); setSuggestions([]); setSelectedSuggestion(null); setDeleted(null); setActiveSlot(0); setPlaying(false); nextNumber.current = 1
      window.requestAnimationFrame(() => fit(loaded))
    } catch (cause) { if (request === loadCounter.current) setError(cause instanceof Error ? cause.message : 'Could not load the image.') }
  }

  function stagePoint(event: React.PointerEvent): Point {
    const rect = stageRef.current!.getBoundingClientRect()
    return { x: event.clientX - rect.left, y: event.clientY - rect.top }
  }

  function updateRegion(next: Region) { setRegions(previous => previous.map(region => region.id === next.id ? next : region)) }

  function startDrag(event: React.PointerEvent, action?: 'move' | Handle, region?: Region) {
    if (!source || !imageSize || event.button > 1) return
    event.preventDefault(); event.stopPropagation()
    stageRef.current?.setPointerCapture(event.pointerId)
    const point = stagePoint(event)
    if (event.button === 1 || mode === 'pan' || spaceHeld) dragRef.current = { kind: 'pan', start: point, pan }
    else if (region && action) { setSelectedId(region.id); dragRef.current = action === 'move' ? { kind: 'move', start: point, region } : { kind: 'resize', start: point, region, handle: action } }
    else {
      const start = screenToImage(point, pan, zoom, imageSize)
      const id = crypto.randomUUID()
      const name = `Frame ${nextNumber.current++}`
      dragRef.current = { kind: 'draw', start, id, name }
      setDraft(regionFromCorners(start, start, id, name))
    }
  }

  function moveDrag(event: React.PointerEvent) {
    const drag = dragRef.current
    if (!drag || !source || !imageSize) return
    const point = stagePoint(event)
    if (drag.kind === 'pan') { setPan({ x: drag.pan.x + point.x - drag.start.x, y: drag.pan.y + point.y - drag.start.y }); return }
    if (drag.kind === 'draw') { setDraft(regionFromCorners(drag.start, screenToImage(point, pan, zoom, imageSize), drag.id, drag.name)); return }
    const dx = Math.round((point.x - drag.start.x) / zoom)
    const dy = Math.round((point.y - drag.start.y) / zoom)
    if (drag.kind === 'move') updateRegion({ ...drag.region, x: clamp(drag.region.x + dx, 0, source.width - drag.region.width), y: clamp(drag.region.y + dy, 0, source.height - drag.region.height) })
    else updateRegion(resizeRegion(drag.region, drag.handle!, dx, dy, imageSize))
  }

  function endDrag(event: React.PointerEvent) {
    const drag = dragRef.current
    if (!drag) return
    if (drag.kind === 'draw' && imageSize) {
      const finished = regionFromCorners(drag.start, screenToImage(stagePoint(event), pan, zoom, imageSize), drag.id, drag.name)
      setRegions(previous => [...previous, finished]); setSelectedId(finished.id)
    }
    setDraft(null); dragRef.current = null
    if (stageRef.current?.hasPointerCapture(event.pointerId)) stageRef.current.releasePointerCapture(event.pointerId)
  }

  function cancelDrag() { dragRef.current = null; setDraft(null) }

  function setMagnification(next: number, anchor?: Point) {
    const stage = stageRef.current
    if (!stage) return
    const point = anchor ?? { x: stage.clientWidth / 2, y: stage.clientHeight / 2 }
    const limited = clamp(next, 0.25, 32)
    setPan(zoomAt(pan, point, zoom, limited)); setZoom(limited)
  }

  function addFrame() {
    if (!source) return
    const width = Math.min(32, source.width), height = Math.min(32, source.height)
    const region: Region = { id: crypto.randomUUID(), name: `Frame ${nextNumber.current++}`, x: Math.floor((source.width - width) / 2), y: Math.floor((source.height - height) / 2), width, height }
    setRegions(previous => [...previous, region]); setSelectedId(region.id)
  }

  function editNumber(key: 'x' | 'y' | 'width' | 'height', raw: string) {
    if (!selected || !imageSize || raw === '') return
    const value = Number(raw)
    if (!Number.isFinite(value)) return
    updateRegion(boundedRegion({ ...selected, [key]: value }, imageSize))
  }

  function editOffset(key: 'x' | 'y', raw: string) {
    if (!assigned[activeSlot] || raw === '') return
    const value = Number(raw)
    if (Number.isFinite(value)) setOffsets(previous => previous.map((offset, index) => index === activeSlot ? { ...offset, [key]: clamp(Math.round(value), -4096, 4096) } : offset))
  }

  function nudge(dx: number, dy: number) {
    if (!assigned[activeSlot]) return
    setOffsets(previous => previous.map((offset, index) => index === activeSlot ? { x: clamp(offset.x + dx, -4096, 4096), y: clamp(offset.y + dy, -4096, 4096) } : offset))
  }

  function moveAlignment(event: React.PointerEvent<HTMLDivElement>) {
    const drag = alignmentDrag.current
    if (!drag) return
    const dx = event.clientX - drag.x, dy = event.clientY - drag.y
    if (drag.kind === 'pan') { setAlignmentPan({ x: drag.offset.x + dx, y: drag.offset.y + dy }); return }
    if (!assigned[drag.slot]) return
    const next = alignmentDragOffset(drag.offset, dx, dy, drag.scale)
    setOffsets(previous => previous.map((offset, index) => index === drag.slot ? next : offset))
  }

  function deleteFrame(id: string) {
    const result = removeFrame({ regions, slots, selectedId }, id)
    if (!result.undo) return
    setRegions(result.state.regions); setSlots(result.state.slots); setSelectedId(result.state.selectedId); setDeleted(result.undo)
    setNotice(`${result.undo.region.name} deleted.`)
  }

  function undoDelete() {
    if (!deleted) return
    const restored = restoreFrame({ regions, slots, selectedId }, deleted)
    setRegions(restored.regions); setSlots(restored.slots); setSelectedId(restored.selectedId); setDeleted(null); setNotice(`${deleted.region.name} restored.`)
  }

  function generateSuggestions() {
    if (!source || !pixels) return
    const next = detectionMode === 'grid' ? gridSuggestions(source, grid) : alphaSuggestions(pixels, { joinGap, minPixels })
    setSuggestions(next); setSelectedSuggestion(next[0]?.id ?? null)
    setNotice(next.length ? `${next.length} suggestions ready for review. Existing frames are unchanged.` : 'No regions found. Adjust the layout or use manual selection.')
  }

  function editSuggestion(key: 'x' | 'y' | 'width' | 'height', raw: string) {
    if (!suggestion || !imageSize || raw === '') return
    const value = Number(raw)
    if (!Number.isFinite(value)) return
    setSuggestions(previous => previous.map(item => item.id === suggestion.id ? boundedRegion({ ...item, [key]: value }, imageSize) : item))
  }

  function accept(id: string) {
    const next = acceptSuggestion(regions, suggestions, id, crypto.randomUUID())
    setRegions(next.regions); setSuggestions(next.suggestions)
    setSelectedId(next.regions.at(-1)?.id ?? selectedId)
    setSelectedSuggestion(next.suggestions[0]?.id ?? null)
  }

  function reject(id: string) {
    const next = rejectSuggestion(suggestions, id)
    setSuggestions(next); setSelectedSuggestion(next[0]?.id ?? null)
  }

  async function exportOne(region: Region) {
    if (!source) return
    setError(null)
    try { await downloadRegion(source.bitmap, region, source.name); setNotice(`Downloaded ${region.name} as a source crop PNG.`) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Export failed.') }
  }

  async function exportAligned() {
    if (!source || !assigned[activeSlot]) return
    setError(null)
    try { await downloadAlignedSlot(source.bitmap, layout, activeSlot, source.name); setNotice(`Downloaded aligned animation slot ${activeSlot + 1}.`) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Export failed.') }
  }

  return <div className="workshop">
    <header className="topbar">
      <div className="brand"><span className="brand-icon" aria-hidden="true">▦</span><span><strong>SPRITE WORKSHOP</strong><small>AiiDE / developer utility</small></span></div>
      <div className="header-actions"><span className="local-badge"><i /> LOCAL ONLY</span><div className="layout-actions"><button className="small" onClick={() => setWorkshopLayout(previous => ({ ...previous, panels: { ...DEFAULT_LAYOUT.panels } }))}>Expand all</button><button className="small" onClick={() => { setAlignmentExpanded(false); setWorkshopLayout(previous => ({ ...previous, panels: Object.fromEntries(PANEL_KEYS.map(key => [key, false])) as PanelVisibility })) }}>Collapse all</button><button className="small" onClick={() => { setAlignmentExpanded(false); setWorkshopLayout({ panels: { ...DEFAULT_LAYOUT.panels }, leftWidth: DEFAULT_LAYOUT.leftWidth, rightWidth: DEFAULT_LAYOUT.rightWidth }) }}>Reset layout</button></div><input ref={fileRef} type="file" accept="image/png,.png" className="visually-hidden" aria-label="Import PNG" onChange={event => { void importFile(event.target.files?.[0]); event.target.value = '' }} /><button className="primary" onClick={() => fileRef.current?.click()}>Import PNG</button></div>
    </header>
    <div className="intro"><div><span className="eyebrow">FRAME EXTRACTION / MILESTONE 02</span><h1>Shape each frame by hand.</h1><p>Map regions, inspect conservative suggestions, and align an eight-frame loop without changing source pixels.</p></div><div className="source-meta"><span>SOURCE</span><strong>{source?.name ?? 'No image loaded'}</strong><small>{source ? `${source.width} × ${source.height} px · PNG` : 'Drop a PNG onto the canvas to begin'}</small></div></div>
    {(error || notice || deleted) && <div className={error ? 'message error' : 'message'} role={error ? 'alert' : 'status'}>{error ?? notice}{deleted && <button className="undo-button" onClick={undoDelete}>Undo deletion of {deleted.region.name}</button>}<button aria-label="Dismiss message" onClick={() => { setError(null); setNotice(null); setDeleted(null) }}>×</button></div>}
    <main className="workspace" style={workspaceStyle}>
      <aside className="panel frames-panel" aria-label="Frames and suggestions"><PanelHeader id="frames" eyebrow="01 / REGIONS" title={`Frame list · ${regions.length}`} expanded={workshopLayout.panels.frames} onToggle={() => togglePanel('frames')} extra={<button className="small" disabled={!source} onClick={addFrame}>+ Add</button>} />
        <div id="panel-frames" className="frames-list" hidden={!workshopLayout.panels.frames}>{regions.length === 0 ? <div className="panel-empty"><span>▧</span><strong>No frames yet</strong><p>Drag across the image, add a centered region, or preview suggestions.</p></div> : regions.map((region, index) => <div key={region.id} className={`frame-row ${selectedId === region.id ? 'active' : ''}`}><button className="frame-select" onClick={() => setSelectedId(region.id)}><Thumbnail image={source!.bitmap} region={region} /><span className="frame-text"><strong>{region.name}</strong><small>{region.width} × {region.height} px · {region.x}, {region.y}</small></span><span className="frame-index">{String(index + 1).padStart(2, '0')}</span></button><button className="row-delete" aria-label={`Delete ${region.name}`} title={`Delete ${region.name}`} onClick={() => deleteFrame(region.id)}>×</button></div>)}</div>
        <DetectionPanel enabled={Boolean(source)} mode={detectionMode} onMode={setDetectionMode} grid={grid} onGrid={setGrid} joinGap={joinGap} onJoinGap={setJoinGap} minPixels={minPixels} onMinPixels={setMinPixels} suggestions={suggestions} selected={suggestion} onSelected={setSelectedSuggestion} onGenerate={generateSuggestions} onEdit={editSuggestion} onAccept={accept} onReject={reject} onClear={() => { setSuggestions([]); setSelectedSuggestion(null) }} />
        <div className="panel-foot">Coordinates always use source-image pixels.</div>
      </aside>
      {resizeHandle('left')}
      <section className="canvas-panel" aria-label="Sprite sheet editor"><div className="canvas-toolbar"><div className="tool-group"><button className={mode === 'select' ? 'tool active' : 'tool'} onClick={() => setMode('select')} aria-pressed={mode === 'select'} title="Draw or edit regions">▣ <span>Select</span></button><button className={mode === 'pan' ? 'tool active' : 'tool'} onClick={() => setMode('pan')} aria-pressed={mode === 'pan'} title="Pan canvas">✥ <span>Pan</span></button></div><span className="toolbar-hint">{source ? 'Drag to draw · drag frame to move · handles to resize · Space or middle drag to pan' : 'Import a PNG to start'}</span><div className="zoom-controls"><button aria-label="Zoom out" disabled={!source} onClick={() => setMagnification(zoom / 1.25)}>−</button><output>{Math.round(zoom * 100)}%</output><button aria-label="Zoom in" disabled={!source} onClick={() => setMagnification(zoom * 1.25)}>+</button><button disabled={!source} onClick={() => fit()} title="Fit image in view">Fit</button></div></div>
        <div ref={stageRef} className={`stage ${dragOver ? 'drag-over' : ''} ${mode === 'pan' || spaceHeld ? 'panning' : ''}`} onPointerDown={event => startDrag(event)} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={cancelDrag} onWheel={event => { if (!source) return; event.preventDefault(); const rect = stageRef.current!.getBoundingClientRect(); setMagnification(zoom * (event.deltaY < 0 ? 1.1 : 1 / 1.1), { x: event.clientX - rect.left, y: event.clientY - rect.top }) }} onDragOver={event => { event.preventDefault(); setDragOver(true) }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragOver(false) }} onDrop={event => { event.preventDefault(); setDragOver(false); void importFile(event.dataTransfer.files[0]) }}>
          {source ? <div className="image-surface checker" style={{ left: pan.x, top: pan.y, width: source.width * zoom, height: source.height * zoom }}><canvas ref={sourceCanvasRef} width={source.width} height={source.height} className="source-canvas" />{suggestions.map(item => <div key={item.id} className={`region suggestion-region ${selectedSuggestion === item.id ? 'focused' : ''}`} style={{ left: item.x * zoom, top: item.y * zoom, width: item.width * zoom, height: item.height * zoom }} onPointerDown={event => { event.stopPropagation(); setSelectedSuggestion(item.id) }}><span className="region-tag">{item.name}</span></div>)}{regions.map(region => <div key={region.id} className={`region ${selectedId === region.id ? 'selected' : ''}`} style={{ left: region.x * zoom, top: region.y * zoom, width: region.width * zoom, height: region.height * zoom }} onPointerDown={event => { setSelectedId(region.id); startDrag(event, 'move', region) }}><span className="region-tag">{region.name}</span>{selectedId === region.id && HANDLES.map(handle => <span key={handle} className={`handle handle-${handle}`} onPointerDown={event => startDrag(event, handle, region)} />)}</div>)}{draft && <div className="region drafting" style={{ left: draft.x * zoom, top: draft.y * zoom, width: draft.width * zoom, height: draft.height * zoom }} />}</div> : <div className="drop-prompt"><div className="drop-mark">▦</div><strong>Drop a PNG to begin</strong><p>Your image stays in this browser. No upload, no server processing.</p><button className="secondary" onClick={() => fileRef.current?.click()}>Choose a file</button></div>}
          {dragOver && <div className="drop-cover">Release to import PNG</div>}
        </div><div className="stage-footer"><span>{source ? `${source.width} × ${source.height} PX` : 'WAITING FOR SOURCE'}</span><span>NEAREST NEIGHBOUR · ORIGINAL PIXELS</span></div>
      </section>
      {resizeHandle('right')}
      <aside className="panel detail-panel" aria-label="Frame and animation controls"><PanelHeader id="inspection" eyebrow="02 / INSPECTOR" title="Frame inspection" expanded={workshopLayout.panels.inspection} onToggle={() => togglePanel('inspection')} />
        <div id="panel-inspection" hidden={!workshopLayout.panels.inspection}>{selected && source ? <div className="details">
          <label className="field"><span>Name</span><input value={selected.name} maxLength={48} onChange={event => updateRegion({ ...selected, name: event.target.value })} /></label>
          <div className="field-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>{key === 'width' ? 'Width' : key === 'height' ? 'Height' : key.toUpperCase()}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} max={key === 'x' ? source.width - 1 : key === 'y' ? source.height - 1 : key === 'width' ? source.width - selected.x : source.height - selected.y} value={selected[key]} onChange={event => editNumber(key, event.target.value)} /></label>)}</div>
          <div className="preview-label">SOURCE CROP <span>{selected.width} × {selected.height} PX</span></div>
          <div className="selected-preview checker"><SpriteCanvas image={source.bitmap} region={selected} width={selected.width} height={selected.height} /></div>
          <div className="detail-actions"><button className="primary" onClick={() => void exportOne(selected)}>Export PNG</button><button className="danger" onClick={() => deleteFrame(selected.id)}>Delete frame</button></div>
          <p className="help">This PNG exports the source crop only. Animation offsets are set and exported separately below.</p>
        </div> : <div className="panel-empty details-empty"><span>◇</span><strong>Select a frame</strong><p>Draw on the canvas or choose a frame from the list to edit its exact coordinates.</p></div>}</div>
        <PanelHeader id="alignment" eyebrow="03 / ANIMATION" title="Animation alignment" expanded={workshopLayout.panels.alignment} onToggle={() => togglePanel('alignment')} />
        <div id="panel-alignment" className="panel-section" hidden={!workshopLayout.panels.alignment}><p className="section-help">Assign crops, then place each slot on one pixel canvas. Drag the preview or use exact offsets.</p>
          <div className="alignment-controls"><label className="field"><span>Initial anchor</span><select value={alignmentMode} onChange={event => setAlignmentMode(event.target.value as AlignmentMode)}><option value="bottom">Bottom centre</option><option value="center">Centre</option></select></label><button className="small" disabled={!assigned.some(Boolean)} onClick={() => setOffsets(emptyOffsets())}>Align all to anchor</button><div className="field-grid"><label className="field"><span>Padding</span><input type="number" min="0" max="256" value={padding} onChange={event => setPadding(clamp(Number(event.target.value) || 0, 0, 256))} /></label><label className="field"><span>Min width</span><input type="number" min="0" max="4096" value={minWidth} onChange={event => setMinWidth(clamp(Number(event.target.value) || 0, 0, 4096))} /></label><label className="field"><span>Min height</span><input type="number" min="0" max="4096" value={minHeight} onChange={event => setMinHeight(clamp(Number(event.target.value) || 0, 0, 4096))} /></label></div><p className="layout-size">Shared canvas: {layout.width} × {layout.height} px</p></div></div>
        <PanelHeader id="preview" title="Animation preview" expanded={workshopLayout.panels.preview} onToggle={() => togglePanel('preview')} />
        <div id="panel-preview" className="panel-section" hidden={!workshopLayout.panels.preview}>
          <div className={alignmentExpanded ? 'alignment-view expanded' : 'alignment-view'}>
            <div className="alignment-toolbar" aria-label="Alignment preview controls">
              {(['fit', 1, 2, 4, 8] as const).map(value => <button key={value} className={alignmentZoom === value ? 'active' : ''} aria-pressed={alignmentZoom === value} onClick={() => { setAlignmentZoom(value); setAlignmentPan({ x: 0, y: 0 }) }}>{value === 'fit' ? 'Fit' : `${value}×`}</button>)}
              <button className={alignmentPanMode ? 'active' : ''} aria-pressed={alignmentPanMode} onClick={() => setAlignmentPanMode(value => !value)}>Pan</button>
              {alignmentExpanded && <><button aria-label="Previous alignment slot" onClick={() => setActiveSlot(index => (index + 7) % 8)}>‹</button><output>Slot {activeSlot + 1} · X {offsets[activeSlot].x} / Y {offsets[activeSlot].y} px</output><button aria-label="Next alignment slot" onClick={() => setActiveSlot(index => (index + 1) % 8)}>›</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(-1, 0)} aria-label="Nudge left one pixel">←</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, -1)} aria-label="Nudge up one pixel">↑</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, 1)} aria-label="Nudge down one pixel">↓</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(1, 0)} aria-label="Nudge right one pixel">→</button></>}
              <button onClick={() => setAlignmentExpanded(value => !value)}>{alignmentExpanded ? 'Close' : 'Expand'}</button>
            </div>
            <div ref={alignmentPreviewRef} className={alignmentPanMode ? 'alignment-preview checker pan-mode' : 'alignment-preview checker'} tabIndex={assigned[activeSlot] ? 0 : -1} role="group" aria-label={`Animation slot ${activeSlot + 1} alignment preview. Drag to align, use Pan to move the view, or press arrow keys to move one source pixel.`} onKeyDown={event => { const directions: Record<string, Point> = { ArrowLeft: { x: -1, y: 0 }, ArrowRight: { x: 1, y: 0 }, ArrowUp: { x: 0, y: -1 }, ArrowDown: { x: 0, y: 1 } }; const direction = directions[event.key]; if (direction) { event.preventDefault(); nudge(direction.x, direction.y) } }} onPointerDown={event => { if (!assigned[activeSlot] || event.button > 1) return; event.preventDefault(); const panning = alignmentPanMode || event.button === 1; if (!panning) setPlaying(false); alignmentDrag.current = { kind: panning ? 'pan' : 'move', x: event.clientX, y: event.clientY, offset: panning ? alignmentPan : offsets[activeSlot], slot: activeSlot, scale: previewScale }; event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.focus() }} onPointerMove={moveAlignment} onPointerUp={event => { alignmentDrag.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }} onPointerCancel={() => { alignmentDrag.current = null }}>
              {source && assigned[activeSlot] ? <div className="alignment-surface" style={{ width: layout.width * previewScale, height: layout.height * previewScale, transform: `translate(calc(-50% + ${alignmentPan.x}px), calc(-50% + ${alignmentPan.y}px))` }}>
                <SpriteCanvas image={source.bitmap} placement={layout.placements[activeSlot]} onionPlacement={onion && !playing && referenceSlot !== activeSlot ? layout.placements[referenceSlot] : null} width={layout.width} height={layout.height} />
                {pixelGrid && previewScale >= 2 && <div className="alignment-grid" style={{ backgroundSize: `${previewScale}px ${previewScale}px` }} />}
                {centreGuide && <div className="alignment-centre" style={{ left: layout.anchorX * previewScale }} />}
                {baselineGuide && <div className="alignment-baseline" style={{ top: baselineY * previewScale }} />}
                <div className="alignment-anchor" style={{ left: layout.anchorX * previewScale, top: layout.anchorY * previewScale }} title={`Anchor ${layout.anchorX}, ${layout.anchorY}`} />
              </div> : <span>{source ? 'Assign a frame below' : 'No source image'}</span>}
            </div>
            <div className="alignment-guide-controls"><label className="check-field"><input type="checkbox" checked={centreGuide} onChange={event => setCentreGuide(event.target.checked)} /> Centre line</label><label className="check-field"><input type="checkbox" checked={baselineGuide} onChange={event => setBaselineGuide(event.target.checked)} /> Baseline</label><label className="check-field"><input type="checkbox" checked={pixelGrid} onChange={event => setPixelGrid(event.target.checked)} /> Pixel grid (2×+)</label><label className="field baseline-field"><span>Baseline Y (canvas px)</span><input type="number" min="0" max={layout.height - 1} value={baselineY} onChange={event => setBaselineOffset(clamp(Number(event.target.value) || 0, 0, layout.height - 1) - layout.anchorY)} /></label></div>
          </div>
        </div>
        <PanelHeader id="offsets" title="Slot offsets" expanded={workshopLayout.panels.offsets} onToggle={() => togglePanel('offsets')} />
        <div id="panel-offsets" className="panel-section" hidden={!workshopLayout.panels.offsets}><div className="alignment-controls"><div className="field-grid offset-fields">{(['x', 'y'] as const).map(key => <label className="field" key={key}><span>Slot {activeSlot + 1} {key.toUpperCase()} offset (px)</span><input type="number" min="-4096" max="4096" disabled={!assigned[activeSlot]} value={offsets[activeSlot][key]} onChange={event => editOffset(key, event.target.value)} /></label>)}</div><div className="nudge-controls"><button disabled={!assigned[activeSlot]} onClick={() => nudge(-1, 0)} aria-label="Nudge left one pixel">←</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, -1)} aria-label="Nudge up one pixel">↑</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, 1)} aria-label="Nudge down one pixel">↓</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(1, 0)} aria-label="Nudge right one pixel">→</button><button disabled={!assigned[activeSlot]} onClick={() => setOffsets(previous => previous.map((offset, index) => index === activeSlot ? { x: 0, y: 0 } : offset))}>Reset slot</button></div><label className="check-field"><input type="checkbox" checked={onion} onChange={event => setOnion(event.target.checked)} /> Onion skin</label><label className="field"><span>Compare with</span><select value={onionReference} disabled={!onion} onChange={event => setOnionReference(event.target.value as 'previous' | 'fixed')}><option value="previous">Previous slot</option><option value="fixed">Fixed slot</option></select></label>{onionReference === 'fixed' && <label className="field"><span>Reference slot</span><select value={fixedReferenceSlot} disabled={!onion} onChange={event => setFixedReferenceSlot(Number(event.target.value))}>{slots.map((id, index) => <option key={index} value={index}>{`Slot ${index + 1}${id ? ` · ${assigned[index]?.name ?? 'Frame'}` : ' · Empty'}`}</option>)}</select></label>}<p className="motion-note">{onion && referenceSlot === activeSlot ? 'Choose another reference slot to see a comparison. ' : ''}Offsets can move pixels outside the canvas. Increase minimum size or padding if artwork is clipped.</p></div>
          <button className="small aligned-export" disabled={!assigned[activeSlot] || !source} onClick={() => void exportAligned()}>Export aligned slot PNG</button></div>
        <PanelHeader id="slots" title="Frame slot list" expanded={workshopLayout.panels.slots} onToggle={() => togglePanel('slots')} />
        <div id="panel-slots" className="panel-section" hidden={!workshopLayout.panels.slots}>
          <div className="play-controls"><button aria-label="Previous frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 7) % 8)}>‹</button><button className="play-button" disabled={!assigned.some(Boolean) || reducedMotion} onClick={() => setPlaying(value => !value)}>{playing ? 'Pause' : 'Play'}</button><button aria-label="Next frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 1) % 8)}>›</button><label>FPS <input type="number" min="1" max="24" value={fps} onChange={event => setFps(clamp(Number(event.target.value) || 1, 1, 24))} /></label></div>
          {reducedMotion && <p className="motion-note">Automatic playback is off because reduced motion is enabled. Step through frames manually.</p>}
          <div className="slot-grid">{slots.map((id, index) => <label key={index} className={activeSlot === index ? 'slot active' : 'slot'}><span>{String(index + 1).padStart(2, '0')}</span><select aria-label={`Animation frame ${index + 1}`} value={id ?? ''} onFocus={() => setActiveSlot(index)} onChange={event => { const value = event.target.value || null; setSlots(previous => previous.map((item, itemIndex) => itemIndex === index ? value : item)); setOffsets(previous => previous.map((offset, itemIndex) => itemIndex === index ? { x: 0, y: 0 } : offset)); setActiveSlot(index) }}><option value="">Empty</option>{regions.map(region => <option key={region.id} value={region.id}>{region.name}</option>)}</select></label>)}</div><button className="fill-button" disabled={!regions.length} onClick={() => { setSlots(Array.from({ length: 8 }, (_, index) => regions[index]?.id ?? null)); setOffsets(emptyOffsets()); setActiveSlot(0) }}>Fill slots from frame list</button>
        </div>
      </aside>
    </main>
  </div>
}

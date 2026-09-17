import { useEffect, useMemo, useRef, useState } from 'react'
import { boundedRegion, clamp, fitZoom, regionFromCorners, resizeRegion, screenToImage, zoomAt } from './geometry'
import type { Handle, Point, Region } from './geometry'
import { downloadRegion } from './export'
import { loadPng } from './image'
import type { SourceImage } from './image'
import { SpriteCanvas, Thumbnail } from './Preview'

type Drag =
  | { kind: 'draw'; start: Point; id: string; name: string }
  | { kind: 'pan'; start: Point; pan: Point }
  | { kind: 'move' | 'resize'; start: Point; region: Region; handle?: Handle }

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']
const EMPTY_SLOTS = Array<string | null>(8).fill(null)

export default function App() {
  const [source, setSource] = useState<SourceImage | null>(null)
  const sourceRef = useRef<SourceImage | null>(null)
  const [regions, setRegions] = useState<Region[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [slots, setSlots] = useState<(string | null)[]>(EMPTY_SLOTS)
  const [activeSlot, setActiveSlot] = useState(0)
  const [fps, setFps] = useState(8)
  const [playing, setPlaying] = useState(false)
  const [reducedMotion, setReducedMotion] = useState(false)
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
  const sourceCanvasRef = useRef<HTMLCanvasElement>(null)
  const dragRef = useRef<Drag | null>(null)
  const nextNumber = useRef(1)
  const loadCounter = useRef(0)
  const selected = regions.find(region => region.id === selectedId) ?? null
  const imageSize = source ? { width: source.width, height: source.height } : null
  const assigned = useMemo(() => slots.map(id => regions.find(region => region.id === id) ?? null), [slots, regions])
  const previewSize = useMemo(() => ({ width: Math.max(1, ...assigned.map(region => region?.width ?? 1)), height: Math.max(1, ...assigned.map(region => region?.height ?? 1)) }), [assigned])

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
      sourceRef.current?.bitmap.close()
      sourceRef.current = loaded
      setSource(loaded); setRegions([]); setSelectedId(null); setSlots([...EMPTY_SLOTS]); setActiveSlot(0); setPlaying(false); nextNumber.current = 1
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

  function deleteFrame() {
    if (!selected) return
    setRegions(previous => previous.filter(region => region.id !== selected.id))
    setSlots(previous => previous.map(id => id === selected.id ? null : id))
    setSelectedId(null)
  }

  async function exportOne(region: Region) {
    if (!source) return
    setError(null)
    try { await downloadRegion(source.bitmap, region, source.name); setNotice(`Downloaded ${region.name} as a transparent PNG.`) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Export failed.') }
  }

  return <div className="workshop">
    <header className="topbar">
      <div className="brand"><span className="brand-icon" aria-hidden="true">▦</span><span><strong>SPRITE WORKSHOP</strong><small>AiiDE / developer utility</small></span></div>
      <div className="header-actions"><span className="local-badge"><i /> LOCAL ONLY</span><input ref={fileRef} type="file" accept="image/png,.png" className="visually-hidden" aria-label="Import PNG" onChange={event => { void importFile(event.target.files?.[0]); event.target.value = '' }} /><button className="primary" onClick={() => fileRef.current?.click()}>Import PNG</button></div>
    </header>
    <div className="intro"><div><span className="eyebrow">FRAME EXTRACTION / MILESTONE 01</span><h1>Shape each frame by hand.</h1><p>Map regions on a sprite sheet, preview an eight-frame loop, and export untouched transparent crops.</p></div><div className="source-meta"><span>SOURCE</span><strong>{source?.name ?? 'No image loaded'}</strong><small>{source ? `${source.width} × ${source.height} px · PNG` : 'Drop a PNG onto the canvas to begin'}</small></div></div>
    {(error || notice) && <div className={error ? 'message error' : 'message'} role={error ? 'alert' : 'status'}>{error ?? notice}<button aria-label="Dismiss message" onClick={() => { setError(null); setNotice(null) }}>×</button></div>}
    <main className="workspace">
      <aside className="panel frames-panel"><div className="panel-head"><div><span className="eyebrow">01 / REGIONS</span><h2>Frames <em>{regions.length}</em></h2></div><button className="small" disabled={!source} onClick={addFrame}>+ Add</button></div>
        <div className="frames-list">{regions.length === 0 ? <div className="panel-empty"><span>▧</span><strong>No frames yet</strong><p>Drag across the image to mark a frame, or add a centered region.</p></div> : regions.map((region, index) => <button key={region.id} className={`frame-row ${selectedId === region.id ? 'active' : ''}`} onClick={() => setSelectedId(region.id)}><Thumbnail image={source!.bitmap} region={region} /><span className="frame-text"><strong>{region.name}</strong><small>{region.width} × {region.height} px · {region.x}, {region.y}</small></span><span className="frame-index">{String(index + 1).padStart(2, '0')}</span></button>)}</div>
        <div className="panel-foot">Coordinates always use source-image pixels.</div>
      </aside>
      <section className="canvas-panel" aria-label="Sprite sheet editor"><div className="canvas-toolbar"><div className="tool-group"><button className={mode === 'select' ? 'tool active' : 'tool'} onClick={() => setMode('select')} aria-pressed={mode === 'select'} title="Draw or edit regions">▣ <span>Select</span></button><button className={mode === 'pan' ? 'tool active' : 'tool'} onClick={() => setMode('pan')} aria-pressed={mode === 'pan'} title="Pan canvas">✥ <span>Pan</span></button></div><span className="toolbar-hint">{source ? 'Drag to draw · drag frame to move · handles to resize · Space or middle drag to pan' : 'Import a PNG to start'}</span><div className="zoom-controls"><button aria-label="Zoom out" disabled={!source} onClick={() => setMagnification(zoom / 1.25)}>−</button><output>{Math.round(zoom * 100)}%</output><button aria-label="Zoom in" disabled={!source} onClick={() => setMagnification(zoom * 1.25)}>+</button><button disabled={!source} onClick={() => fit()} title="Fit image in view">Fit</button></div></div>
        <div ref={stageRef} className={`stage ${dragOver ? 'drag-over' : ''} ${mode === 'pan' || spaceHeld ? 'panning' : ''}`} onPointerDown={event => startDrag(event)} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={cancelDrag} onWheel={event => { if (!source) return; event.preventDefault(); const rect = stageRef.current!.getBoundingClientRect(); setMagnification(zoom * (event.deltaY < 0 ? 1.1 : 1 / 1.1), { x: event.clientX - rect.left, y: event.clientY - rect.top }) }} onDragOver={event => { event.preventDefault(); setDragOver(true) }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragOver(false) }} onDrop={event => { event.preventDefault(); setDragOver(false); void importFile(event.dataTransfer.files[0]) }}>
          {source ? <div className="image-surface checker" style={{ left: pan.x, top: pan.y, width: source.width * zoom, height: source.height * zoom }}><canvas ref={sourceCanvasRef} width={source.width} height={source.height} className="source-canvas" />{regions.map(region => <div key={region.id} className={`region ${selectedId === region.id ? 'selected' : ''}`} style={{ left: region.x * zoom, top: region.y * zoom, width: region.width * zoom, height: region.height * zoom }} onPointerDown={event => { setSelectedId(region.id); startDrag(event, 'move', region) }}><span className="region-tag">{region.name}</span>{selectedId === region.id && HANDLES.map(handle => <span key={handle} className={`handle handle-${handle}`} onPointerDown={event => startDrag(event, handle, region)} />)}</div>)}{draft && <div className="region drafting" style={{ left: draft.x * zoom, top: draft.y * zoom, width: draft.width * zoom, height: draft.height * zoom }} />}</div> : <div className="drop-prompt"><div className="drop-mark">▦</div><strong>Drop a PNG to begin</strong><p>Your image stays in this browser. No upload, no server processing.</p><button className="secondary" onClick={() => fileRef.current?.click()}>Choose a file</button></div>}
          {dragOver && <div className="drop-cover">Release to import PNG</div>}
        </div><div className="stage-footer"><span>{source ? `${source.width} × ${source.height} PX` : 'WAITING FOR SOURCE'}</span><span>NEAREST NEIGHBOUR · ORIGINAL PIXELS</span></div>
      </section>
      <aside className="panel detail-panel"><div className="panel-head"><div><span className="eyebrow">02 / INSPECTOR</span><h2>Frame details</h2></div></div>{selected && source ? <div className="details"><label className="field"><span>Name</span><input value={selected.name} maxLength={48} onChange={event => updateRegion({ ...selected, name: event.target.value })} /></label><div className="field-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>{key === 'width' ? 'Width' : key === 'height' ? 'Height' : key.toUpperCase()}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} max={key === 'x' ? source.width - 1 : key === 'y' ? source.height - 1 : key === 'width' ? source.width - selected.x : source.height - selected.y} value={selected[key]} onChange={event => editNumber(key, event.target.value)} /></label>)}</div><div className="preview-label">SELECTED FRAME <span>{selected.width} × {selected.height} PX</span></div><div className="selected-preview checker"><SpriteCanvas image={source.bitmap} region={selected} width={selected.width} height={selected.height} /></div><div className="detail-actions"><button className="primary" onClick={() => void exportOne(selected)}>Export PNG</button><button className="danger" onClick={deleteFrame}>Delete frame</button></div><p className="help">The exported PNG contains only pixels inside this rectangle. Move the edges inside any labels or numbers on the source sheet.</p></div> : <div className="panel-empty details-empty"><span>◇</span><strong>Select a frame</strong><p>Draw on the canvas or choose a frame from the list to edit its exact coordinates.</p></div>}
        <div className="animation"><div className="animation-head"><span className="eyebrow">03 / MOTION TEST</span><h2>Eight-frame loop</h2><p>Assign any saved region to each slot. All frames use one shared preview canvas.</p></div><div className="animation-preview checker">{source && assigned[activeSlot] ? <SpriteCanvas image={source.bitmap} region={assigned[activeSlot]!} width={previewSize.width} height={previewSize.height} /> : <span>{source ? 'Assign a frame below' : 'No source image'}</span>}</div><div className="play-controls"><button aria-label="Previous frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 7) % 8)}>‹</button><button className="play-button" disabled={!assigned.some(Boolean) || reducedMotion} onClick={() => setPlaying(value => !value)}>{playing ? 'Pause' : 'Play'}</button><button aria-label="Next frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 1) % 8)}>›</button><label>FPS <input type="number" min="1" max="24" value={fps} onChange={event => setFps(clamp(Number(event.target.value) || 1, 1, 24))} /></label></div>{reducedMotion && <p className="motion-note">Automatic playback is off because reduced motion is enabled. Step through frames manually.</p>}<div className="slot-grid">{slots.map((id, index) => <label key={index} className={activeSlot === index ? 'slot active' : 'slot'}><span>{String(index + 1).padStart(2, '0')}</span><select aria-label={`Animation frame ${index + 1}`} value={id ?? ''} onFocus={() => setActiveSlot(index)} onChange={event => { const value = event.target.value || null; setSlots(previous => previous.map((item, itemIndex) => itemIndex === index ? value : item)); setActiveSlot(index) }}><option value="">Empty</option>{regions.map(region => <option key={region.id} value={region.id}>{region.name}</option>)}</select></label>)}</div><button className="fill-button" disabled={!regions.length} onClick={() => { setSlots(Array.from({ length: 8 }, (_, index) => regions[index]?.id ?? null)); setActiveSlot(0) }}>Fill slots from frame list</button></div>
      </aside>
    </main>
  </div>
}

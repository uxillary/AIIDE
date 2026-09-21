import { useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent } from 'react'
import { boundedRegion, clamp, fitZoom, regionFromCorners, resizeRegion, screenToImage, zoomAt } from './geometry'
import type { Handle, Point, Region } from './geometry'
import { downloadAlignedSlot, downloadAnimation, downloadRegion, validateAnimation } from './export'
import { loadPng } from './image'
import type { SourceImage } from './image'
import { SpriteCanvas, Thumbnail, TouchUpEditor } from './Preview'
import { alignmentDragOffset, alignmentScale, calculateLayout, emptyOffsets, referenceSlotFor } from './alignment'
import type { AlignmentMode, AlignmentZoom, Pixels } from './alignment'
import { acceptAllSuggestions, acceptSuggestion, acceptSuggestionBatch, alphaSuggestions, animationRowSuggestions, eligibleSuggestionCount, gridSuggestions, rejectSuggestion } from './detection'
import type { GridOptions, RowBoundaryMode, RowSuggestion } from './detection'
import { removeFrame, restoreFrame, slotsFromFrameOrder } from './frameActions'
import type { DeletedFrame } from './frameActions'
import { DetectionPanel } from './DetectionPanel'
import { deserializePortableProject, loadLatestProject, PROJECT_SCHEMA_VERSION, projectFileName, restoreSource, saveLatestProject, saveWithStatus, serializePortableProject } from './project'
import type { ProjectData, SourceRecord, StoredProject } from './project'
import { appendErase, resetFrameTouchUps, stepTouchUpHistory } from './touchups'
import type { EraseRect, TouchUps } from './touchups'

type Drag =
  | { kind: 'draw'; start: Point; id: string; name: string }
  | { kind: 'row-draw'; start: Point }
  | { kind: 'pan'; start: Point; pan: Point }
  | { kind: 'move' | 'resize' | 'row-move' | 'row-resize'; start: Point; region: Region; handle?: Handle }

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']
const EMPTY_SLOTS = Array<string | null>(8).fill(null)
const WORKFLOW_TABS = [
  { id: 'extract', label: '01 Extract' },
  { id: 'inspect', label: '02 Inspect' },
  { id: 'animate', label: '03 Animate' },
  { id: 'export', label: '04 Export' },
] as const
type WorkflowTab = typeof WORKFLOW_TABS[number]['id']
type WorkshopLayout = { rightWidth: number }
const DEFAULT_LAYOUT: WorkshopLayout = { rightWidth: 380 }
const LAYOUT_STORAGE_KEY = 'sprite-workshop-layout-v2'

function readLayout(): WorkshopLayout {
  try {
    const saved = JSON.parse(window.localStorage.getItem(LAYOUT_STORAGE_KEY) ?? 'null') as Partial<WorkshopLayout> | null
    if (!saved || typeof saved !== 'object') return DEFAULT_LAYOUT
    return {
      rightWidth: typeof saved.rightWidth === 'number' && Number.isFinite(saved.rightWidth) ? clamp(saved.rightWidth, 320, 560) : DEFAULT_LAYOUT.rightWidth,
    }
  } catch { return DEFAULT_LAYOUT }
}

export default function App() {
  const [workshopLayout, setWorkshopLayout] = useState(readLayout)
  const [activeTab, setActiveTab] = useState<WorkflowTab>('extract')
  const [viewportWidth, setViewportWidth] = useState(() => window.innerWidth)
  const [source, setSource] = useState<SourceImage | null>(null)
  const [sourceRecord, setSourceRecord] = useState<SourceRecord | null>(null)
  const sourceRef = useRef<SourceImage | null>(null)
  const [regions, setRegions] = useState<Region[]>([])
  const [pixels, setPixels] = useState<Pixels | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [deleted, setDeleted] = useState<DeletedFrame | null>(null)
  const [touchUps, setTouchUps] = useState<TouchUps>({})
  const [touchUpUndo, setTouchUpUndo] = useState<TouchUps[]>([])
  const [touchUpRedo, setTouchUpRedo] = useState<TouchUps[]>([])
  const [eraseMode, setEraseMode] = useState(false)
  const [touchUpZoom, setTouchUpZoom] = useState(4)
  const [suggestions, setSuggestions] = useState<RowSuggestion[]>([])
  const [selectedSuggestion, setSelectedSuggestion] = useState<string | null>(null)
  const [detectionMode, setDetectionMode] = useState<'grid' | 'alpha' | 'row'>('grid')
  const [grid, setGrid] = useState<GridOptions>({ rows: 5, columns: 8, gapX: 8, gapY: 8, left: 0, right: 0, top: 0, bottom: 0 })
  const [joinGap, setJoinGap] = useState(12)
  const [minPixels, setMinPixels] = useState(8)
  const [rowSelection, setRowSelection] = useState<Region | null>(null)
  const [rowFrameCount, setRowFrameCount] = useState(8)
  const [rowBoundaryMode, setRowBoundaryMode] = useState<RowBoundaryMode>('equal')
  const [rowPadding, setRowPadding] = useState(0)
  const [assignBatch, setAssignBatch] = useState(false)
  const [slots, setSlots] = useState<(string | null)[]>(EMPTY_SLOTS)
  const [activeSlot, setActiveSlot] = useState(0)
  const [fps, setFps] = useState(8)
  const [animationName, setAnimationName] = useState('animation')
  const [exporting, setExporting] = useState(false)
  const [exportError, setExportError] = useState<string | null>(null)
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
  const [saveStatus, setSaveStatus] = useState<'loading' | 'unsaved' | 'saving' | 'saved' | 'failed'>('loading')
  const [saveError, setSaveError] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const projectFileRef = useRef<HTMLInputElement>(null)
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([])
  const stageRef = useRef<HTMLDivElement>(null)
  const alignmentPreviewRef = useRef<HTMLDivElement>(null)
  const sourceCanvasRef = useRef<HTMLCanvasElement>(null)
  const dragRef = useRef<Drag | null>(null)
  const alignmentDrag = useRef<{ kind: 'move' | 'pan'; x: number; y: number; offset: Point; slot: number; scale: number } | null>(null)
  const nextNumber = useRef(1)
  const loadCounter = useRef(0)
  const hydrated = useRef(false)
  const restoreStarted = useRef(false)
  const saveTimer = useRef<number | null>(null)
  const pendingSave = useRef<StoredProject | null>(null)
  const saveRevision = useRef(0)
  const savePromise = useRef<Promise<void>>(Promise.resolve())
  const selected = regions.find(region => region.id === selectedId) ?? null
  const suggestion = suggestions.find(region => region.id === selectedSuggestion) ?? null
  const imageSize = source ? { width: source.width, height: source.height } : null
  const assigned = useMemo(() => slots.map(id => regions.find(region => region.id === id) ?? null), [slots, regions])
  const layout = useMemo(() => calculateLayout(assigned, offsets, { mode: alignmentMode, padding, minWidth, minHeight }), [assigned, offsets, alignmentMode, padding, minWidth, minHeight])
  const exportIssue = source ? validateAnimation(slots, regions, layout, animationName, fps) : 'Import a PNG before exporting.'
  const referenceSlot = referenceSlotFor(activeSlot, onionReference, fixedReferenceSlot)
  const previewScale = alignmentScale(alignmentZoom, layout, alignmentViewport)
  const baselineY = clamp(layout.anchorY + baselineOffset, 0, layout.height - 1)
  const eligibleSuggestions = imageSize ? eligibleSuggestionCount(regions, suggestions, imageSize) : 0
  const activePlacement = layout.placements[activeSlot]
  const activeSlotClipped = Boolean(activePlacement && (activePlacement.x < 0 || activePlacement.y < 0 || activePlacement.x + activePlacement.source.width > layout.width || activePlacement.y + activePlacement.source.height > layout.height))
  const rightWidth = Math.min(workshopLayout.rightWidth, Math.max(320, viewportWidth - 480))
  const workspaceStyle = { '--right-width': `${rightWidth}px` } as CSSProperties
  const projectData = useMemo<ProjectData>(() => ({
    schemaVersion: PROJECT_SCHEMA_VERSION, regions, selectedId, slots, activeSlot, fps, animationName, alignmentMode, offsets, padding, minWidth, minHeight,
    onion, onionReference, fixedReferenceSlot, centreGuide, baselineGuide, pixelGrid, baselineOffset, detectionMode, grid, joinGap, minPixels,
    rowSelection, rowFrameCount, rowBoundaryMode, rowPadding, touchUps,
  }), [regions, selectedId, slots, activeSlot, fps, animationName, alignmentMode, offsets, padding, minWidth, minHeight, onion, onionReference, fixedReferenceSlot, centreGuide, baselineGuide, pixelGrid, baselineOffset, detectionMode, grid, joinGap, minPixels, rowSelection, rowFrameCount, rowBoundaryMode, rowPadding, touchUps])

  function applyProject(project: ProjectData) {
    setRegions(project.regions); setSelectedId(project.selectedId); setSlots(project.slots); setActiveSlot(project.activeSlot); setFps(project.fps); setAnimationName(project.animationName)
    setAlignmentMode(project.alignmentMode); setOffsets(project.offsets); setPadding(project.padding); setMinWidth(project.minWidth); setMinHeight(project.minHeight)
    setOnion(project.onion); setOnionReference(project.onionReference); setFixedReferenceSlot(project.fixedReferenceSlot); setCentreGuide(project.centreGuide); setBaselineGuide(project.baselineGuide)
    setPixelGrid(project.pixelGrid); setBaselineOffset(project.baselineOffset); setDetectionMode(project.detectionMode); setGrid(project.grid); setJoinGap(project.joinGap); setMinPixels(project.minPixels)
    setRowSelection(project.rowSelection ?? null); setRowFrameCount(project.rowFrameCount ?? 8); setRowBoundaryMode(project.rowBoundaryMode ?? 'equal'); setRowPadding(project.rowPadding ?? 0)
    setTouchUps(project.touchUps ?? {}); setTouchUpUndo([]); setTouchUpRedo([]); setEraseMode(false)
    setSuggestions([]); setSelectedSuggestion(null); setDeleted(null); setPlaying(false); setExportError(null)
    nextNumber.current = project.regions.length + 1
  }

  async function applyStoredProject(record: StoredProject) {
    const restored = await restoreSource(record.source)
    sourceRef.current?.bitmap.close(); sourceRef.current = restored.source
    setSource(restored.source); setPixels(restored.pixels); setSourceRecord({ blob: restored.blob, ...restored.metadata }); applyProject(record.project)
    window.requestAnimationFrame(() => fit(restored.source))
  }

  function makeRecord(data = projectData): StoredProject | null {
    return sourceRecord ? { id: 'latest', schemaVersion: PROJECT_SCHEMA_VERSION, savedAt: Date.now(), project: data, source: sourceRecord } : null
  }

  function commitSave(record: StoredProject, revision: number): Promise<void> {
    setSaveError(null)
    const task = saveWithStatus(() => saveLatestProject(record), value => {
      if (saveRevision.current === revision) setSaveStatus(value)
    }).then(() => {
      if (saveRevision.current === revision) pendingSave.current = null
    }).catch(cause => {
      if (saveRevision.current === revision) setSaveError(`${cause instanceof Error ? cause.message : 'Local save failed.'} Keep this tab open and export a project file as a backup.`)
      throw cause
    })
    savePromise.current = task.catch(() => undefined)
    return task
  }

  async function flushPendingSave(): Promise<boolean> {
    if (saveTimer.current !== null) { window.clearTimeout(saveTimer.current); saveTimer.current = null }
    const record = pendingSave.current
    if (!record) { await savePromise.current; return saveStatus !== 'failed' }
    const revision = saveRevision.current
    try { await commitSave(record, revision); return true } catch { return false }
  }

  function selectTab(tab: WorkflowTab) {
    setAlignmentExpanded(false)
    setActiveTab(tab)
  }

  function handleTabKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>, index: number) {
    let nextIndex = index
    if (event.key === 'ArrowLeft') nextIndex = (index + WORKFLOW_TABS.length - 1) % WORKFLOW_TABS.length
    else if (event.key === 'ArrowRight') nextIndex = (index + 1) % WORKFLOW_TABS.length
    else if (event.key === 'Home') nextIndex = 0
    else if (event.key === 'End') nextIndex = WORKFLOW_TABS.length - 1
    else return
    event.preventDefault()
    selectTab(WORKFLOW_TABS[nextIndex].id)
    tabRefs.current[nextIndex]?.focus()
  }

  function resizeSidebar(width: number) {
    setWorkshopLayout({ rightWidth: clamp(Math.round(width), 320, Math.max(320, Math.min(560, window.innerWidth - 480))) })
  }

  function resizeHandle() {
    const width = rightWidth
    return <div className="sidebar-resizer" role="separator" tabIndex={0} aria-label="Resize tool area" aria-orientation="vertical" aria-valuemin={320} aria-valuemax={560} aria-valuenow={width}
      onPointerDown={event => { event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.dataset.startX = String(event.clientX); event.currentTarget.dataset.startWidth = String(width) }}
      onPointerMove={event => { if (!event.currentTarget.hasPointerCapture(event.pointerId)) return; const delta = event.clientX - Number(event.currentTarget.dataset.startX); resizeSidebar(Number(event.currentTarget.dataset.startWidth) - delta) }}
      onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}
      onKeyDown={event => { const direction = event.key === 'ArrowLeft' ? 1 : event.key === 'ArrowRight' ? -1 : 0; if (!direction && event.key !== 'Home' && event.key !== 'End') return; event.preventDefault(); resizeSidebar(event.key === 'Home' ? 320 : event.key === 'End' ? 560 : width + direction * (event.shiftKey ? 25 : 10)) }} />
  }

  useEffect(() => {
    if (restoreStarted.current) return
    restoreStarted.current = true
    void loadLatestProject().then(async record => {
      if (record) { await applyStoredProject(record); setSaveStatus('saved'); setNotice('Restored the most recent local project.') }
      else setSaveStatus('unsaved')
    }).catch(cause => { setSaveStatus('failed'); setSaveError(`${cause instanceof Error ? cause.message : 'Could not open local project storage.'} You can still work and export a project backup.`) }).finally(() => { hydrated.current = true })
  // Startup recovery intentionally runs once; subsequent project loads are explicit actions.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (!hydrated.current || !sourceRecord) return
    const record: StoredProject = { id: 'latest', schemaVersion: PROJECT_SCHEMA_VERSION, savedAt: Date.now(), project: projectData, source: sourceRecord }
    const revision = ++saveRevision.current
    pendingSave.current = record
    setSaveStatus('unsaved'); setSaveError(null)
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current)
    saveTimer.current = window.setTimeout(() => { saveTimer.current = null; void commitSave(record, revision).catch(() => undefined) }, 600)
  }, [projectData, sourceRecord])

  useEffect(() => {
    const flushWhenHidden = () => { if (document.visibilityState === 'hidden') void flushPendingSave() }
    const flushOnPageHide = () => { void flushPendingSave() }
    document.addEventListener('visibilitychange', flushWhenHidden); window.addEventListener('pagehide', flushOnPageHide)
    return () => { document.removeEventListener('visibilitychange', flushWhenHidden); window.removeEventListener('pagehide', flushOnPageHide); if (saveTimer.current !== null) window.clearTimeout(saveTimer.current) }
  // The listeners read the latest pending-save refs and must only be registered once.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

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
  }, [alignmentExpanded, activeTab])

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
  }, [source, activeTab])
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
    if (source && ['unsaved', 'saving', 'failed'].includes(saveStatus) && !window.confirm('Discard unsaved changes and import a different source PNG?')) return
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
      setSource(loaded); setSourceRecord({ blob: file, name: file.name, type: 'image/png', size: file.size, lastModified: file.lastModified }); setPixels({ data: imageData.data, width: loaded.width, height: loaded.height }); setRegions([]); setSelectedId(null); setSlots([...EMPTY_SLOTS]); setOffsets(emptyOffsets()); setSuggestions([]); setSelectedSuggestion(null); setRowSelection(null); setDeleted(null); setTouchUps({}); setTouchUpUndo([]); setTouchUpRedo([]); setEraseMode(false); setActiveSlot(0); setPlaying(false); nextNumber.current = 1
      setAnimationName(loaded.name.replace(/\.png$/i, '') || 'animation'); setExportError(null)
      window.requestAnimationFrame(() => fit(loaded))
    } catch (cause) { if (request === loadCounter.current) setError(cause instanceof Error ? cause.message : 'Could not load the image.') }
  }

  function stagePoint(event: React.PointerEvent): Point {
    const rect = stageRef.current!.getBoundingClientRect()
    return { x: event.clientX - rect.left, y: event.clientY - rect.top }
  }

  function updateRegion(next: Region) {
    setRegions(previous => previous.map(region => region.id === next.id ? next : region))
    const valid = (touchUps[next.id] ?? []).filter(rect => rect.x + rect.width <= next.width && rect.y + rect.height <= next.height)
    if (valid.length !== (touchUps[next.id]?.length ?? 0)) {
      setTouchUps(valid.length ? { ...touchUps, [next.id]: valid } : resetFrameTouchUps(touchUps, next.id)); setTouchUpUndo([]); setTouchUpRedo([])
    }
  }

  function startDrag(event: React.PointerEvent, action?: 'move' | Handle, region?: Region, row = false) {
    if (!source || !imageSize || event.button > 1) return
    event.preventDefault(); event.stopPropagation()
    stageRef.current?.setPointerCapture(event.pointerId)
    const point = stagePoint(event)
    if (event.button === 1 || mode === 'pan' || spaceHeld) dragRef.current = { kind: 'pan', start: point, pan }
    else if (region && action && row) dragRef.current = action === 'move' ? { kind: 'row-move', start: point, region } : { kind: 'row-resize', start: point, region, handle: action }
    else if (region && action) { setSelectedId(region.id); dragRef.current = action === 'move' ? { kind: 'move', start: point, region } : { kind: 'resize', start: point, region, handle: action } }
    else {
      const start = screenToImage(point, pan, zoom, imageSize)
      if (detectionMode === 'row') { dragRef.current = { kind: 'row-draw', start }; setDraft(regionFromCorners(start, start, 'row-selection', 'Animation row')); return }
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
    if (drag.kind === 'draw' || drag.kind === 'row-draw') { setDraft(regionFromCorners(drag.start, screenToImage(point, pan, zoom, imageSize), drag.kind === 'draw' ? drag.id : 'row-selection', drag.kind === 'draw' ? drag.name : 'Animation row')); return }
    const dx = Math.round((point.x - drag.start.x) / zoom)
    const dy = Math.round((point.y - drag.start.y) / zoom)
    if (drag.kind === 'move') updateRegion({ ...drag.region, x: clamp(drag.region.x + dx, 0, source.width - drag.region.width), y: clamp(drag.region.y + dy, 0, source.height - drag.region.height) })
    else if (drag.kind === 'resize') updateRegion(resizeRegion(drag.region, drag.handle!, dx, dy, imageSize))
    else if (drag.kind === 'row-move') setRowSelection({ ...drag.region, x: clamp(drag.region.x + dx, 0, source.width - drag.region.width), y: clamp(drag.region.y + dy, 0, source.height - drag.region.height) })
    else setRowSelection(resizeRegion(drag.region, drag.handle!, dx, dy, imageSize))
  }

  function endDrag(event: React.PointerEvent) {
    const drag = dragRef.current
    if (!drag) return
    if ((drag.kind === 'draw' || drag.kind === 'row-draw') && imageSize) {
      const finished = regionFromCorners(drag.start, screenToImage(stagePoint(event), pan, zoom, imageSize), drag.kind === 'draw' ? drag.id : 'row-selection', drag.kind === 'draw' ? drag.name : 'Animation row')
      if (drag.kind === 'row-draw') { setRowSelection(finished); setSuggestions([]); setSelectedSuggestion(null) }
      else { setRegions(previous => [...previous, finished]); setSelectedId(finished.id) }
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

  function selectAdjacentFrame(direction: -1 | 1) {
    if (!regions.length) return
    const current = regions.findIndex(region => region.id === selectedId)
    const index = current < 0 ? 0 : (current + direction + regions.length) % regions.length
    setSelectedId(regions[index].id)
  }

  function fitSelected() {
    if (!selected || !stageRef.current) return
    const stage = stageRef.current
    const next = clamp(Math.min((stage.clientWidth - 80) / selected.width, (stage.clientHeight - 80) / selected.height), 0.25, 32)
    setZoom(next); setPan({ x: (stage.clientWidth - selected.width * next) / 2 - selected.x * next, y: (stage.clientHeight - selected.height * next) / 2 - selected.y * next })
  }

  function fillSlotsFromFrames() {
    const result = slotsFromFrameOrder(regions, slots)
    if (result.replacesAssignments && !window.confirm('Replace the existing animation slot assignments with the current frame-list order?')) return
    setSlots(result.slots); setOffsets(emptyOffsets()); setActiveSlot(0)
    setNotice(`Filled ${Math.min(regions.length, 8)} animation slot${Math.min(regions.length, 8) === 1 ? '' : 's'} from the frame list.`)
  }

  function resetProject() {
    sourceRef.current?.bitmap.close(); sourceRef.current = null; setSource(null); setSourceRecord(null); setPixels(null); setRegions([]); setSelectedId(null); setSlots([...EMPTY_SLOTS]); setOffsets(emptyOffsets()); setTouchUps({}); setTouchUpUndo([]); setTouchUpRedo([]); setEraseMode(false)
    setSuggestions([]); setSelectedSuggestion(null); setRowSelection(null); setRowFrameCount(8); setRowBoundaryMode('equal'); setRowPadding(0); setAssignBatch(false); setDeleted(null); setActiveSlot(0); setPlaying(false); setFps(8); setAnimationName('animation'); setAlignmentMode('bottom'); setPadding(8); setMinWidth(0); setMinHeight(0)
    setOnion(false); setOnionReference('previous'); setFixedReferenceSlot(0); setCentreGuide(true); setBaselineGuide(true); setPixelGrid(false); setBaselineOffset(0); setAlignmentZoom('fit'); setAlignmentPan({ x: 0, y: 0 })
    setDetectionMode('grid'); setGrid({ rows: 5, columns: 8, gapX: 8, gapY: 8, left: 0, right: 0, top: 0, bottom: 0 }); setJoinGap(12); setMinPixels(8); setZoom(1); setPan({ x: 0, y: 0 }); setMode('select'); setActiveTab('extract')
    setError(null); setNotice('Started a new project.'); setSaveError(null); setSaveStatus('unsaved'); nextNumber.current = 1
  }

  function newProject() {
    if (['unsaved', 'saving', 'failed'].includes(saveStatus) && source && !window.confirm('Discard unsaved changes and start a new project?')) return
    if (saveTimer.current !== null) { window.clearTimeout(saveTimer.current); saveTimer.current = null }
    pendingSave.current = null; saveRevision.current += 1; resetProject()
  }

  async function reopenRecentProject() {
    if (['unsaved', 'saving', 'failed'].includes(saveStatus) && source && !window.confirm('Discard unsaved changes and reopen the most recent saved project?')) return
    setError(null)
    try {
      const record = await loadLatestProject()
      if (!record) { setNotice('No locally saved project is available yet.'); return }
      await applyStoredProject(record); setSaveStatus('saved'); setSaveError(null); setNotice('Reopened the most recent local project.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Could not reopen the local project.') }
  }

  async function exportProjectFile() {
    const record = makeRecord()
    if (!record) { setError('Import a source PNG before exporting a project.'); return }
    await flushPendingSave()
    try {
      const text = await serializePortableProject(record)
      const url = URL.createObjectURL(new Blob([text], { type: 'application/json' })); const anchor = document.createElement('a')
      anchor.href = url; anchor.download = projectFileName(animationName); document.body.append(anchor)
      try { anchor.click() } finally { anchor.remove(); window.setTimeout(() => URL.revokeObjectURL(url), 30_000) }
      setNotice('Exported a self-contained project file.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Project export failed.') }
  }

  async function importProjectFile(file?: File) {
    if (!file) return
    setError(null)
    try {
      const record = deserializePortableProject(await file.text())
      await applyStoredProject(record)
      const revision = ++saveRevision.current; pendingSave.current = record
      await commitSave(record, revision)
      setNotice('Imported and saved the project locally.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Project import failed. The current session was not changed.') }
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
    setTouchUps(previous => resetFrameTouchUps(previous, id)); setTouchUpUndo([]); setTouchUpRedo([])
    setNotice(`${result.undo.region.name} deleted.`)
  }

  function undoDelete() {
    if (!deleted) return
    const restored = restoreFrame({ regions, slots, selectedId }, deleted)
    setRegions(restored.regions); setSlots(restored.slots); setSelectedId(restored.selectedId); setDeleted(null); setNotice(`${deleted.region.name} restored.`)
  }

  function commitTouchUps(next: TouchUps, message: string) {
    if (next === touchUps) { setNotice('This frame has reached the 512 rectangle touch-up limit.'); return }
    setTouchUpUndo(previous => [...previous.slice(-99), touchUps]); setTouchUpRedo([]); setTouchUps(next); setNotice(message)
  }

  function erasePixels(rect: EraseRect) {
    if (!selected) return
    commitTouchUps(appendErase(touchUps, selected.id, rect), `Erased ${rect.width} × ${rect.height} px from ${selected.name}.`)
  }

  function undoTouchUp() {
    const step = stepTouchUpHistory(touchUpUndo, touchUpRedo, touchUps)
    if (!step) return
    setTouchUpUndo(step.from); setTouchUpRedo(step.to); setTouchUps(step.touchUps); setNotice('Undid touch-up edit.')
  }

  function redoTouchUp() {
    const step = stepTouchUpHistory(touchUpRedo, touchUpUndo, touchUps)
    if (!step) return
    setTouchUpRedo(step.from); setTouchUpUndo(step.to); setTouchUps(step.touchUps); setNotice('Redid touch-up edit.')
  }

  function resetSelectedTouchUps() {
    if (!selected || !(selected.id in touchUps)) return
    commitTouchUps(resetFrameTouchUps(touchUps, selected.id), `Reset touch-ups for ${selected.name}.`)
  }

  function generateSuggestions() {
    if (!source || !pixels) return
    const next: RowSuggestion[] = detectionMode === 'grid' ? gridSuggestions(source, grid) : detectionMode === 'alpha' ? alphaSuggestions(pixels, { joinGap, minPixels }) : rowSelection ? animationRowSuggestions(pixels, { selection: rowSelection, frameCount: rowFrameCount, boundaryMode: rowBoundaryMode, padding: rowPadding }) : []
    setSuggestions(next); setSelectedSuggestion(next[0]?.id ?? null)
    const uncertain = next.filter(item => item.needsReview).length
    setNotice(next.length ? `${next.length} suggestions ready for review. Existing frames are unchanged.${uncertain ? ` ${uncertain} need manual review.` : ''}` : 'No regions found. Adjust the selection or settings, or use manual selection.')
  }

  function editRowSelection(key: 'x' | 'y' | 'width' | 'height', raw: string) {
    if (!rowSelection || !imageSize || raw === '') return
    const value = Number(raw)
    if (!Number.isFinite(value)) return
    setRowSelection(boundedRegion({ ...rowSelection, [key]: value }, imageSize)); setSuggestions([]); setSelectedSuggestion(null)
  }

  function editSuggestion(key: 'x' | 'y' | 'width' | 'height', raw: string) {
    if (!suggestion || !imageSize || raw === '') return
    const value = Number(raw)
    if (!Number.isFinite(value)) return
    setSuggestions(previous => previous.map(item => item.id === suggestion.id ? boundedRegion({ ...item, [key]: value }, imageSize) : item))
  }

  function accept(id: string) {
    if (!imageSize) return
    const next = acceptSuggestion(regions, suggestions, id, crypto.randomUUID(), imageSize)
    if (next.regions === regions) { setNotice('Suggestion was not added because it is invalid, duplicates an existing region, or the 256-frame limit was reached.'); return }
    setRegions(next.regions); setSuggestions(next.suggestions)
    setSelectedId(next.regions.at(-1)?.id ?? selectedId)
    setSelectedSuggestion(next.suggestions[0]?.id ?? null)
  }

  function acceptAll() {
    if (!imageSize || eligibleSuggestions === 0) return
    if (detectionMode === 'row') {
      const next = acceptSuggestionBatch(regions, suggestions, imageSize, () => crypto.randomUUID())
      if (next.error) { setNotice(`No frames were added. ${next.error}`); return }
      let nextSlots = slots
      if (assignBatch) {
        const assignment = slotsFromFrameOrder(next.added, slots)
        if (assignment.replacesAssignments && !window.confirm('Replace the existing animation slot assignments with this accepted row?')) return
        nextSlots = assignment.slots
      }
      setRegions(next.regions); setSuggestions([]); setSelectedSuggestion(null); setSelectedId(next.added[0]?.id ?? selectedId)
      if (assignBatch) { setSlots(nextSlots); setOffsets(emptyOffsets()); setActiveSlot(0) }
      setNotice(`Added ${next.added.length} ordered animation frames${assignBatch ? ' and assigned them to slots' : ''}.`)
      return
    }
    const next = acceptAllSuggestions(regions, suggestions, imageSize, () => crypto.randomUUID())
    setRegions(next.regions); setSuggestions(next.suggestions)
    if (next.added) setSelectedId(next.regions.at(-1)?.id ?? selectedId)
    setSelectedSuggestion(null)
    setNotice(`${next.added} suggestion${next.added === 1 ? '' : 's'} added, ${next.skipped} skipped as duplicate${next.skipped === 1 ? '' : 's'}, ${next.rejected} rejected as invalid or over the frame limit.`)
  }

  function reject(id: string) {
    const next = rejectSuggestion(suggestions, id)
    setSuggestions(next); setSelectedSuggestion(next[0]?.id ?? null)
  }

  async function exportOne(region: Region) {
    if (!source) return
    setError(null)
    try { await downloadRegion(source.bitmap, region, source.name, touchUps); setNotice(`Downloaded ${region.name} as an edited crop PNG.`) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Export failed.') }
  }

  async function exportAligned() {
    if (!source || !assigned[activeSlot]) return
    setError(null)
    try { await downloadAlignedSlot(source.bitmap, layout, activeSlot, source.name, touchUps); setNotice(`Downloaded aligned animation slot ${activeSlot + 1}.`) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Export failed.') }
  }

  async function exportAnimation() {
    if (!source || exporting) return
    setExportError(null); setError(null); setExporting(true)
    try { await downloadAnimation(source.bitmap, slots, regions, layout, animationName, fps, touchUps); setNotice('Downloaded animation PNG sprite sheet and JSON metadata.') }
    catch (cause) { setExportError(cause instanceof Error ? cause.message : 'Animation export failed.') }
    finally { setExporting(false) }
  }

  return <div className="workshop">
    <header className="topbar">
      <div className="brand"><span className="brand-icon" aria-hidden="true">▦</span><span><strong>SPRITE WORKSHOP</strong><small>AiiDE / developer utility</small></span></div>
      <div className="header-actions"><span className={`save-status ${saveStatus}`} role="status" aria-live="polite" title={saveError ?? undefined}>{saveStatus === 'loading' ? 'Opening local project…' : saveStatus === 'unsaved' ? 'Unsaved changes' : saveStatus === 'saving' ? 'Saving…' : saveStatus === 'saved' ? 'Saved locally' : 'Save failed'}</span><span className="local-badge"><i /> LOCAL ONLY</span><input ref={fileRef} type="file" accept="image/png,.png" className="visually-hidden" aria-label="Import PNG" onChange={event => { void importFile(event.target.files?.[0]); event.target.value = '' }} /><input ref={projectFileRef} type="file" accept=".json,.spriteworkshop.json,application/json" className="visually-hidden" aria-label="Import project file" onChange={event => { void importProjectFile(event.target.files?.[0]); event.target.value = '' }} /><button className="small" onClick={newProject}>New</button><button className="small" onClick={() => void reopenRecentProject()}>Reopen recent</button><button className="small" disabled={!source} onClick={() => void exportProjectFile()}>Export project</button><button className="small" onClick={() => projectFileRef.current?.click()}>Import project</button><button className="primary" onClick={() => fileRef.current?.click()}>Import PNG</button></div>
    </header>
    <div className="intro"><div><span className="eyebrow">FRAME EXTRACTION WORKSPACE</span><h1>Shape each frame by hand.</h1><p>Map regions, align an eight-frame loop, and export a transparent sprite sheet with metadata.</p></div><div className="source-meta"><span>SOURCE</span><strong>{source?.name ?? 'No image loaded'}</strong><small>{source ? `${source.width} × ${source.height} px · PNG` : 'Drop a PNG onto the canvas to begin'}</small></div></div>
    {(error || notice || deleted || saveError) && <div className={(error || saveError) ? 'message error' : 'message'} role={(error || saveError) ? 'alert' : 'status'}>{error ?? saveError ?? notice}{deleted && <button className="undo-button" onClick={undoDelete}>Undo deletion of {deleted.region.name}</button>}<button aria-label="Dismiss message" onClick={() => { setError(null); setNotice(null); setDeleted(null); setSaveError(null) }}>×</button></div>}
    <nav className="workflow-tabs" role="tablist" aria-label="Sprite workflow">{WORKFLOW_TABS.map((tab, index) => <button key={tab.id} ref={element => { tabRefs.current[index] = element }} id={`workflow-tab-${tab.id}`} role="tab" aria-selected={activeTab === tab.id} aria-controls={`workflow-panel-${tab.id}`} tabIndex={activeTab === tab.id ? 0 : -1} onClick={() => selectTab(tab.id)} onKeyDown={event => handleTabKeyDown(event, index)}>{tab.label}</button>)}</nav>
    <main className={activeTab === 'animate' ? 'workspace animate-workspace' : 'workspace'} style={workspaceStyle}>
      <aside id="workflow-panel-extract" className="panel frames-panel" role="tabpanel" aria-labelledby="workflow-tab-extract" hidden={activeTab !== 'extract'}><div className="section-heading"><div><span className="eyebrow">REGIONS</span><h2>Frame library <em>{regions.length}</em></h2></div><button className="small" disabled={!source} onClick={addFrame}>+ Add</button></div>
        <div className="frames-list">{regions.length === 0 ? <div className="panel-empty"><span>▧</span><strong>No frames yet</strong><p>Drag across the image, add a centered region, or preview suggestions.</p></div> : regions.map((region, index) => <div key={region.id} className={`frame-row ${selectedId === region.id ? 'active' : ''}`}><button className="frame-select" onClick={() => setSelectedId(region.id)}><Thumbnail image={source!.bitmap} region={region} touchUps={touchUps} /><span className="frame-text"><strong>{region.name}</strong><small>{region.width} × {region.height} px · {region.x}, {region.y}</small></span><span className="frame-index">{String(index + 1).padStart(2, '0')}</span></button><button className="row-delete" aria-label={`Delete ${region.name}`} title={`Delete ${region.name}`} onClick={() => deleteFrame(region.id)}>×</button></div>)}</div>
        <DetectionPanel enabled={Boolean(source)} image={source?.bitmap ?? null} mode={detectionMode} onMode={value => { setDetectionMode(value); setSuggestions([]); setSelectedSuggestion(null) }} grid={grid} onGrid={setGrid} joinGap={joinGap} onJoinGap={setJoinGap} minPixels={minPixels} onMinPixels={setMinPixels} rowSelection={rowSelection} onRowEdit={editRowSelection} rowFrameCount={rowFrameCount} onRowFrameCount={setRowFrameCount} rowBoundaryMode={rowBoundaryMode} onRowBoundaryMode={setRowBoundaryMode} rowPadding={rowPadding} onRowPadding={setRowPadding} assignBatch={assignBatch} onAssignBatch={setAssignBatch} suggestions={suggestions} selected={suggestion} eligibleCount={eligibleSuggestions} onSelected={setSelectedSuggestion} onGenerate={generateSuggestions} onEdit={editSuggestion} onAccept={accept} onAcceptAll={acceptAll} onReject={reject} onClear={() => { setSuggestions([]); setSelectedSuggestion(null) }} />
        <div className="panel-foot">Coordinates always use source-image pixels.</div>
      </aside>
      <section className="canvas-panel" aria-label="Sprite sheet editor"><div className="canvas-toolbar"><div className="tool-group"><button className={mode === 'select' ? 'tool active' : 'tool'} onClick={() => setMode('select')} aria-pressed={mode === 'select'} title="Draw or edit regions">▣ <span>Select</span></button><button className={mode === 'pan' ? 'tool active' : 'tool'} onClick={() => setMode('pan')} aria-pressed={mode === 'pan'} title="Pan canvas">✥ <span>Pan</span></button></div><span className="toolbar-hint">{source ? detectionMode === 'row' ? 'Drag to select one animation row · Space or middle drag to pan' : 'Drag to draw · drag frame to move · handles to resize · Space or middle drag to pan' : 'Import a PNG to start'}</span><div className="zoom-controls"><button aria-label="Zoom out" disabled={!source} onClick={() => setMagnification(zoom / 1.25)}>−</button><output>{Math.round(zoom * 100)}%</output><button aria-label="Zoom in" disabled={!source} onClick={() => setMagnification(zoom * 1.25)}>+</button><button disabled={!source} onClick={() => fit()} title="Fit image in view">Fit</button></div></div>
        <div ref={stageRef} className={`stage ${dragOver ? 'drag-over' : ''} ${mode === 'pan' || spaceHeld ? 'panning' : ''}`} onPointerDown={event => startDrag(event)} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={cancelDrag} onWheel={event => { if (!source) return; event.preventDefault(); const rect = stageRef.current!.getBoundingClientRect(); setMagnification(zoom * (event.deltaY < 0 ? 1.1 : 1 / 1.1), { x: event.clientX - rect.left, y: event.clientY - rect.top }) }} onDragOver={event => { event.preventDefault(); setDragOver(true) }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragOver(false) }} onDrop={event => { event.preventDefault(); setDragOver(false); void importFile(event.dataTransfer.files[0]) }}>
          {source ? <div className="image-surface checker" style={{ left: pan.x, top: pan.y, width: source.width * zoom, height: source.height * zoom }}><canvas ref={sourceCanvasRef} width={source.width} height={source.height} className="source-canvas" />{suggestions.map(item => <div key={item.id} className={`region suggestion-region ${selectedSuggestion === item.id ? 'focused' : ''} ${item.needsReview ? 'uncertain' : ''}`} style={{ left: item.x * zoom, top: item.y * zoom, width: item.width * zoom, height: item.height * zoom }} onPointerDown={event => { event.stopPropagation(); setSelectedSuggestion(item.id) }}><span className="region-tag">{item.name}{item.needsReview ? ' ⚠' : ''}</span></div>)}{regions.map(region => <div key={region.id} className={`region ${selectedId === region.id ? 'selected' : ''}`} style={{ left: region.x * zoom, top: region.y * zoom, width: region.width * zoom, height: region.height * zoom }} onPointerDown={event => { setSelectedId(region.id); startDrag(event, 'move', region) }}><span className="region-tag">{region.name}</span>{selectedId === region.id && HANDLES.map(handle => <span key={handle} className={`handle handle-${handle}`} onPointerDown={event => startDrag(event, handle, region)} />)}</div>)}{detectionMode === 'row' && rowSelection && <div className="region row-selection" style={{ left: rowSelection.x * zoom, top: rowSelection.y * zoom, width: rowSelection.width * zoom, height: rowSelection.height * zoom }} onPointerDown={event => startDrag(event, 'move', rowSelection, true)}><span className="region-tag">Animation row</span>{HANDLES.map(handle => <span key={handle} className={`handle handle-${handle}`} onPointerDown={event => startDrag(event, handle, rowSelection, true)} />)}</div>}{draft && <div className={`region drafting ${detectionMode === 'row' ? 'row-selection' : ''}`} style={{ left: draft.x * zoom, top: draft.y * zoom, width: draft.width * zoom, height: draft.height * zoom }} />}</div> : <div className="drop-prompt"><div className="drop-mark">▦</div><strong>Drop a PNG to begin</strong><p>Your image stays in this browser. No upload, no server processing.</p><button className="secondary" onClick={() => fileRef.current?.click()}>Choose a file</button></div>}
          {dragOver && <div className="drop-cover">Release to import PNG</div>}
        </div><div className="stage-footer"><span>{source ? `${source.width} × ${source.height} PX` : 'WAITING FOR SOURCE'}</span><span>NEAREST NEIGHBOUR · ORIGINAL PIXELS</span></div>
      </section>
      {resizeHandle()}
      <aside className="panel detail-panel" aria-label="Workflow tools">
        <div id="workflow-panel-inspect" className="workflow-panel" role="tabpanel" aria-labelledby="workflow-tab-inspect" hidden={activeTab !== 'inspect'}><section className="inspect-suggestions"><span className="eyebrow">ORGANISE</span><h2>Frame order and assignments</h2><p>Slots 1–8 follow the current frame-list order. Existing assignments are confirmed before replacement.</p><button className="primary" disabled={!regions.length} onClick={fillSlotsFromFrames}>Fill slots from frame list</button><div className="inspect-frame-strip" aria-label="Frame thumbnails">{regions.map((region, index) => <button key={region.id} className={selectedId === region.id ? 'active' : ''} aria-label={`Select ${region.name}`} aria-pressed={selectedId === region.id} onClick={() => setSelectedId(region.id)}>{source && <Thumbnail image={source.bitmap} region={region} touchUps={touchUps} />}<span>{index + 1}</span></button>)}</div></section>{selected && source ? <div className="details" onKeyDown={event => { const target = event.target; if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'z' || target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) return; event.preventDefault(); if (event.shiftKey) redoTouchUp(); else undoTouchUp() }}>
          <div className="section-heading"><div><span className="eyebrow">SELECTED FRAME</span><h2>Crop inspection</h2></div></div>
          <div className="inspect-nav"><button className="small" onClick={() => selectAdjacentFrame(-1)}>← Previous</button><button className="small" onClick={fitSelected}>Fit selected frame</button><button className="small" onClick={() => selectAdjacentFrame(1)}>Next →</button></div>
          <label className="field"><span>Name</span><input value={selected.name} maxLength={48} onChange={event => updateRegion({ ...selected, name: event.target.value })} /></label>
          <div className="field-grid">{(['x', 'y', 'width', 'height'] as const).map(key => <label className="field" key={key}><span>{key === 'width' ? 'Width' : key === 'height' ? 'Height' : key.toUpperCase()}</span><input type="number" min={key === 'width' || key === 'height' ? 1 : 0} max={key === 'x' ? source.width - 1 : key === 'y' ? source.height - 1 : key === 'width' ? source.width - selected.x : source.height - selected.y} value={selected[key]} onChange={event => editNumber(key, event.target.value)} /></label>)}</div>
          <div className="preview-label">EDITED CROP <span>{selected.width} × {selected.height} PX</span></div>
          <div className="touchup-toolbar" aria-label="Pixel touch-up controls"><button className={eraseMode ? 'tool active' : 'tool'} aria-pressed={eraseMode} onClick={() => setEraseMode(value => !value)}>▧ <span>Rectangle Erase</span></button><button className="small" disabled={!touchUpUndo.length} onClick={undoTouchUp} aria-keyshortcuts="Control+Z">Undo</button><button className="small" disabled={!touchUpRedo.length} onClick={redoTouchUp} aria-keyshortcuts="Control+Shift+Z">Redo</button><button className="small" disabled={!touchUps[selected.id]?.length} onClick={resetSelectedTouchUps}>Reset touch-ups</button></div>
          <div className="touchup-zoom"><span>Zoom</span>{[1, 2, 4, 8].map(value => <button key={value} className={touchUpZoom === value ? 'active' : ''} aria-pressed={touchUpZoom === value} onClick={() => setTouchUpZoom(value)}>{value}×</button>)}</div>
          <div className={eraseMode ? 'selected-preview touchup-preview checker editing' : 'selected-preview touchup-preview checker'}><TouchUpEditor image={source.bitmap} region={selected} erases={touchUps[selected.id] ?? []} active={eraseMode} zoom={touchUpZoom} onErase={erasePixels} /></div>
          <p className="touchup-status" role="status">{eraseMode ? `Rectangle Erase active · ${touchUps[selected.id]?.length ?? 0} edit${touchUps[selected.id]?.length === 1 ? '' : 's'} · drag pixels or use arrows, Shift+arrows and Enter` : `${touchUps[selected.id]?.length ?? 0} touch-up edit${touchUps[selected.id]?.length === 1 ? '' : 's'} · source PNG unchanged`}</p>
          <div className="detail-actions"><button className="danger" onClick={() => deleteFrame(selected.id)}>Delete frame</button></div>
          <p className="help">Touch-ups use frame-local pixels and never alter the source PNG or crop coordinates. Animation placement is edited in Animate.</p>
        </div> : <div className="panel-empty details-empty"><span>◇</span><strong>{source ? 'No frame selected' : 'No source image'}</strong><p>{source ? 'Choose a frame thumbnail above or create one in Extract.' : 'Import a PNG or reopen a local project to inspect frames.'}</p></div>}</div>
        <div id="workflow-panel-animate" className="workflow-panel" role="tabpanel" aria-labelledby="workflow-tab-animate" hidden={activeTab !== 'animate'}>
        <section className="tool-section animate-alignment"><div className="section-heading"><div><span className="eyebrow">ALIGNMENT</span><h2>Animation alignment</h2></div></div><p className="section-help">Assign crops, then place each slot on one pixel canvas. Drag the preview or use exact offsets.</p>
          <div className="alignment-controls"><label className="field"><span>Initial anchor</span><select value={alignmentMode} onChange={event => setAlignmentMode(event.target.value as AlignmentMode)}><option value="bottom">Bottom centre</option><option value="center">Centre</option></select></label><button className="small" disabled={!assigned.some(Boolean)} onClick={() => setOffsets(emptyOffsets())}>Align all to anchor</button><div className="field-grid"><label className="field"><span>Padding</span><input type="number" min="0" max="256" value={padding} onChange={event => setPadding(clamp(Number(event.target.value) || 0, 0, 256))} /></label><label className="field"><span>Min width</span><input type="number" min="0" max="4096" value={minWidth} onChange={event => setMinWidth(clamp(Number(event.target.value) || 0, 0, 4096))} /></label><label className="field"><span>Min height</span><input type="number" min="0" max="4096" value={minHeight} onChange={event => setMinHeight(clamp(Number(event.target.value) || 0, 0, 4096))} /></label></div><p className="layout-size">Shared canvas: {layout.width} × {layout.height} px</p><label className="check-field"><input type="checkbox" checked={onion} onChange={event => setOnion(event.target.checked)} /> Onion skin</label><label className="field"><span>Compare with</span><select value={onionReference} disabled={!onion} onChange={event => setOnionReference(event.target.value as 'previous' | 'fixed')}><option value="previous">Previous slot</option><option value="fixed">Fixed slot</option></select></label>{onionReference === 'fixed' && <label className="field"><span>Reference slot</span><select value={fixedReferenceSlot} disabled={!onion} onChange={event => setFixedReferenceSlot(Number(event.target.value))}>{slots.map((id, index) => <option key={index} value={index}>{`Slot ${index + 1}${id ? ` · ${assigned[index]?.name ?? 'Frame'}` : ' · Empty'}`}</option>)}</select></label>}</div>
          <div className={alignmentExpanded ? 'alignment-view expanded' : 'alignment-view'}>
            <div className="alignment-toolbar" aria-label="Alignment preview controls">
              {(['fit', 1, 2, 4, 8] as const).map(value => <button key={value} className={alignmentZoom === value ? 'active' : ''} aria-pressed={alignmentZoom === value} onClick={() => { setAlignmentZoom(value); setAlignmentPan({ x: 0, y: 0 }) }}>{value === 'fit' ? 'Fit' : `${value}×`}</button>)}
              <button className={alignmentPanMode ? 'active' : ''} aria-pressed={alignmentPanMode} onClick={() => setAlignmentPanMode(value => !value)}>Pan</button>
              {alignmentExpanded && <><button aria-label="Previous alignment slot" onClick={() => setActiveSlot(index => (index + 7) % 8)}>‹</button><output>Slot {activeSlot + 1} · X {offsets[activeSlot].x} / Y {offsets[activeSlot].y} px</output><button aria-label="Next alignment slot" onClick={() => setActiveSlot(index => (index + 1) % 8)}>›</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(-1, 0)} aria-label="Nudge left one pixel">←</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, -1)} aria-label="Nudge up one pixel">↑</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, 1)} aria-label="Nudge down one pixel">↓</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(1, 0)} aria-label="Nudge right one pixel">→</button></>}
              <button onClick={() => setAlignmentExpanded(value => !value)}>{alignmentExpanded ? 'Close' : 'Expand'}</button>
            </div>
            <div ref={alignmentPreviewRef} className={alignmentPanMode ? 'alignment-preview checker pan-mode' : 'alignment-preview checker'} tabIndex={assigned[activeSlot] ? 0 : -1} role="group" aria-label={`Animation slot ${activeSlot + 1} alignment preview. Drag to align, use Pan to move the view, or press arrow keys to move one source pixel.`} onKeyDown={event => { const directions: Record<string, Point> = { ArrowLeft: { x: -1, y: 0 }, ArrowRight: { x: 1, y: 0 }, ArrowUp: { x: 0, y: -1 }, ArrowDown: { x: 0, y: 1 } }; const direction = directions[event.key]; if (direction) { event.preventDefault(); nudge(direction.x, direction.y) } }} onPointerDown={event => { if (!assigned[activeSlot] || event.button > 1) return; event.preventDefault(); const panning = alignmentPanMode || event.button === 1; if (!panning) setPlaying(false); alignmentDrag.current = { kind: panning ? 'pan' : 'move', x: event.clientX, y: event.clientY, offset: panning ? alignmentPan : offsets[activeSlot], slot: activeSlot, scale: previewScale }; event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.focus() }} onPointerMove={moveAlignment} onPointerUp={event => { alignmentDrag.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }} onPointerCancel={() => { alignmentDrag.current = null }}>
              {source && assigned[activeSlot] ? <div className="alignment-surface" style={{ width: layout.width * previewScale, height: layout.height * previewScale, transform: `translate(calc(-50% + ${alignmentPan.x}px), calc(-50% + ${alignmentPan.y}px))` }}>
                <SpriteCanvas image={source.bitmap} placement={layout.placements[activeSlot]} onionPlacement={onion && !playing && referenceSlot !== activeSlot ? layout.placements[referenceSlot] : null} width={layout.width} height={layout.height} touchUps={touchUps} />
                {pixelGrid && previewScale >= 2 && <div className="alignment-grid" style={{ backgroundSize: `${previewScale}px ${previewScale}px` }} />}
                {centreGuide && <div className="alignment-centre" style={{ left: layout.anchorX * previewScale }} />}
                {baselineGuide && <div className="alignment-baseline" style={{ top: baselineY * previewScale }} />}
                <div className="alignment-anchor" style={{ left: layout.anchorX * previewScale, top: layout.anchorY * previewScale }} title={`Anchor ${layout.anchorX}, ${layout.anchorY}`} />
              </div> : <span>{source ? 'Assign a frame below' : 'No source image'}</span>}
            </div>
            <div className="alignment-guide-controls"><label className="check-field"><input type="checkbox" checked={centreGuide} onChange={event => setCentreGuide(event.target.checked)} /> Centre line</label><label className="check-field"><input type="checkbox" checked={baselineGuide} onChange={event => setBaselineGuide(event.target.checked)} /> Baseline</label><label className="check-field"><input type="checkbox" checked={pixelGrid} onChange={event => setPixelGrid(event.target.checked)} /> Pixel grid (2×+)</label><label className="field baseline-field"><span>Baseline Y (canvas px)</span><input type="number" min="0" max={layout.height - 1} value={baselineY} onChange={event => setBaselineOffset(clamp(Number(event.target.value) || 0, 0, layout.height - 1) - layout.anchorY)} /></label></div>
          </div>
        </section>
        <section className="tool-section position-panel"><div className="section-heading"><div><span className="eyebrow">ACTIVE SLOT {String(activeSlot + 1).padStart(2, '0')}</span><h2>Position</h2></div></div><div className="alignment-controls"><p className="active-frame-name">{assigned[activeSlot]?.name ?? 'Empty slot'}</p><div className="field-grid offset-fields">{(['x', 'y'] as const).map(key => <label className="field" key={key}><span>Slot {activeSlot + 1} {key.toUpperCase()} offset (px)</span><input type="number" min="-4096" max="4096" disabled={!assigned[activeSlot]} value={offsets[activeSlot][key]} onChange={event => editOffset(key, event.target.value)} /></label>)}</div><div className="nudge-controls"><button disabled={!assigned[activeSlot]} onClick={() => nudge(-1, 0)} aria-label="Nudge left one pixel">←</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, -1)} aria-label="Nudge up one pixel">↑</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(0, 1)} aria-label="Nudge down one pixel">↓</button><button disabled={!assigned[activeSlot]} onClick={() => nudge(1, 0)} aria-label="Nudge right one pixel">→</button><button disabled={!assigned[activeSlot]} onClick={() => setOffsets(previous => previous.map((offset, index) => index === activeSlot ? { x: 0, y: 0 } : offset))}>Reset slot</button></div><p className={activeSlotClipped ? 'clipping-warning' : 'bounds-status'}>{activeSlotClipped ? 'Warning: this frame is clipped by the shared canvas bounds.' : `Within ${layout.width} × ${layout.height} px canvas bounds.`}</p><p className="motion-note">{onion && referenceSlot === activeSlot ? 'Choose another reference slot to see a comparison. ' : ''}Offsets can move pixels outside the canvas. Increase minimum size or padding if artwork is clipped.</p></div></section>
        <section className="tool-section timeline"><div className="section-heading"><div><span className="eyebrow">8 FRAME LOOP</span><h2>Frame timeline</h2></div></div>
          <div className="play-controls"><button aria-label="Previous frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 7) % 8)}>‹</button><button className="play-button" disabled={!assigned.some(Boolean) || reducedMotion} onClick={() => setPlaying(value => !value)}>{playing ? 'Pause' : 'Play'}</button><button aria-label="Next frame" disabled={!source} onClick={() => setActiveSlot(index => (index + 1) % 8)}>›</button><label>FPS <input type="number" min="1" max="24" value={fps} onChange={event => setFps(clamp(Number(event.target.value) || 1, 1, 24))} /></label></div>
          {reducedMotion && <p className="motion-note">Automatic playback is off because reduced motion is enabled. Step through frames manually.</p>}
          <div className="slot-grid">{slots.map((id, index) => <label key={index} className={activeSlot === index ? 'slot active' : 'slot'}><span>{String(index + 1).padStart(2, '0')}</span><select aria-label={`Animation frame ${index + 1}`} value={id ?? ''} onFocus={() => setActiveSlot(index)} onChange={event => { const value = event.target.value || null; setSlots(previous => previous.map((item, itemIndex) => itemIndex === index ? value : item)); setOffsets(previous => previous.map((offset, itemIndex) => itemIndex === index ? { x: 0, y: 0 } : offset)); setActiveSlot(index) }}><option value="">Empty</option>{regions.map(region => <option key={region.id} value={region.id}>{region.name}</option>)}</select></label>)}</div>
        </section>
        </div>
        <div id="workflow-panel-export" className="workflow-panel export-panel" role="tabpanel" aria-labelledby="workflow-tab-export" hidden={activeTab !== 'export'}>
          <section className="tool-section"><div className="section-heading"><div><span className="eyebrow">INDIVIDUAL ASSETS</span><h2>Frame export</h2></div></div><p className="section-help">Export the selected source crop, or the active animation slot on its aligned canvas.</p><div className="export-actions"><button className="secondary" disabled={!selected || !source} onClick={() => selected && void exportOne(selected)}>Export selected crop PNG</button><button className="secondary" disabled={!assigned[activeSlot] || !source} onClick={() => void exportAligned()}>Export slot {activeSlot + 1} aligned PNG</button></div></section>
          <section className="tool-section"><div className="section-heading"><div><span className="eyebrow">COMPLETE ANIMATION</span><h2>Sprite sheet and metadata</h2></div></div>
          <label className="field"><span>Animation name</span><input value={animationName} maxLength={100} onChange={event => { setAnimationName(event.target.value); setExportError(null) }} /></label>
          <p className="export-summary">{slots.filter(Boolean).length}/8 frames · {fps} FPS · {layout.width * 8} × {layout.height} px sheet<br />Horizontal · {layout.width} × {layout.height} px per frame</p>
          <p className="section-help">Downloads a transparent PNG and matching JSON. Frames are ordered by slot, with zero-based sheet rectangles and a duration of 1000 / FPS milliseconds.</p>
          {(exportError || exportIssue) && <p className="export-error" role="alert">{exportError ?? exportIssue}</p>}
          <button className="primary export-button" disabled={!source || exporting} onClick={() => void exportAnimation()}>{exporting ? 'Preparing export…' : 'Export Animation · PNG + JSON'}</button></section>
        </div>
      </aside>
    </main>
  </div>
}

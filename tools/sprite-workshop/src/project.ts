import type { AlignmentMode, Pixels } from './alignment'
import type { GridOptions, RowBoundaryMode } from './detection'
import type { Point, Region } from './geometry'
import { loadPng } from './image'
import type { SourceImage } from './image'

export const PROJECT_SCHEMA_VERSION = 1
export const PORTABLE_PROJECT_KIND = 'aiide-sprite-workshop-project'

export type ProjectData = {
  schemaVersion: typeof PROJECT_SCHEMA_VERSION
  regions: Region[]
  selectedId: string | null
  slots: (string | null)[]
  activeSlot: number
  fps: number
  animationName: string
  alignmentMode: AlignmentMode
  offsets: Point[]
  padding: number
  minWidth: number
  minHeight: number
  onion: boolean
  onionReference: 'previous' | 'fixed'
  fixedReferenceSlot: number
  centreGuide: boolean
  baselineGuide: boolean
  pixelGrid: boolean
  baselineOffset: number
  detectionMode: 'grid' | 'alpha' | 'row'
  grid: GridOptions
  joinGap: number
  minPixels: number
  rowSelection?: Region | null
  rowFrameCount?: number
  rowBoundaryMode?: RowBoundaryMode
  rowPadding?: number
}

export type SourceRecord = { blob: Blob; name: string; type: string; size: number; lastModified: number }
export type StoredProject = { id: 'latest'; schemaVersion: typeof PROJECT_SCHEMA_VERSION; savedAt: number; project: ProjectData; source: SourceRecord }
type PortableProject = { kind: typeof PORTABLE_PROJECT_KIND; schemaVersion: typeof PROJECT_SCHEMA_VERSION; exportedAt: string; project: ProjectData; source: Omit<SourceRecord, 'blob'> & { dataUrl: string } }
export type RestoredSource = { source: SourceImage; pixels: Pixels; blob: Blob; metadata: Omit<SourceRecord, 'blob'> }

function isNumber(value: unknown): value is number { return typeof value === 'number' && Number.isFinite(value) }
function isPoint(value: unknown): value is Point { return Boolean(value && typeof value === 'object' && isNumber((value as Point).x) && isNumber((value as Point).y)) }
function isRegion(value: unknown): value is Region {
  if (!value || typeof value !== 'object') return false
  const item = value as Region
  return typeof item.id === 'string' && item.id.length > 0 && typeof item.name === 'string' && ['x', 'y', 'width', 'height'].every(key => isNumber(item[key as keyof Region])) && item.width > 0 && item.height > 0
}

export function validateProject(value: unknown): ProjectData {
  if (!value || typeof value !== 'object') throw new Error('Project data is missing or malformed.')
  const project = value as Partial<ProjectData>
  if (project.schemaVersion !== PROJECT_SCHEMA_VERSION) throw new Error(`Unsupported project schema version: ${String(project.schemaVersion)}. Your existing data was not changed.`)
  if (!Array.isArray(project.regions) || project.regions.length > 256 || !project.regions.every(isRegion)) throw new Error('Project frames are malformed.')
  const ids = new Set(project.regions.map(region => region.id))
  if (ids.size !== project.regions.length) throw new Error('Project frame IDs must be unique.')
  if (!Array.isArray(project.slots) || project.slots.length !== 8 || !project.slots.every(id => id === null || (typeof id === 'string' && ids.has(id)))) throw new Error('Project slot assignments are malformed.')
  if (!Array.isArray(project.offsets) || project.offsets.length !== 8 || !project.offsets.every(isPoint)) throw new Error('Project alignment offsets are malformed.')
  if (!isNumber(project.fps) || project.fps < 1 || project.fps > 24 || !isNumber(project.activeSlot) || project.activeSlot < 0 || project.activeSlot > 7) throw new Error('Project playback settings are malformed.')
  if (!['bottom', 'center'].includes(project.alignmentMode ?? '') || !['previous', 'fixed'].includes(project.onionReference ?? '') || !['grid', 'alpha', 'row'].includes(project.detectionMode ?? '')) throw new Error('Project mode settings are malformed.')
  if (!project.grid || !['rows', 'columns', 'gapX', 'gapY', 'left', 'right', 'top', 'bottom'].every(key => isNumber(project.grid?.[key as keyof GridOptions]))) throw new Error('Project detection settings are malformed.')
  for (const key of ['padding', 'minWidth', 'minHeight', 'fixedReferenceSlot', 'baselineOffset', 'joinGap', 'minPixels'] as const) if (!isNumber(project[key])) throw new Error('Project numeric settings are malformed.')
  for (const key of ['onion', 'centreGuide', 'baselineGuide', 'pixelGrid'] as const) if (typeof project[key] !== 'boolean') throw new Error('Project display settings are malformed.')
  if (project.rowSelection !== undefined && project.rowSelection !== null && !isRegion(project.rowSelection)) throw new Error('Project animation row selection is malformed.')
  if (project.rowFrameCount !== undefined && (!isNumber(project.rowFrameCount) || project.rowFrameCount < 1 || project.rowFrameCount > 64)) throw new Error('Project animation row frame count is malformed.')
  if (project.rowBoundaryMode !== undefined && !['equal', 'content'].includes(project.rowBoundaryMode)) throw new Error('Project animation row boundary mode is malformed.')
  if (project.rowPadding !== undefined && (!isNumber(project.rowPadding) || project.rowPadding < 0 || project.rowPadding > 256)) throw new Error('Project animation row padding is malformed.')
  if (typeof project.animationName !== 'string' || (project.selectedId !== null && (typeof project.selectedId !== 'string' || !ids.has(project.selectedId)))) throw new Error('Project identity settings are malformed.')
  return project as ProjectData
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed.')) })
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => { transaction.oncomplete = () => resolve(); transaction.onabort = () => reject(transaction.error ?? new Error('IndexedDB transaction was aborted.')); transaction.onerror = () => reject(transaction.error ?? new Error('IndexedDB transaction failed.')) })
}

async function openDatabase(): Promise<IDBDatabase> {
  if (!('indexedDB' in globalThis)) throw new Error('Local project storage is unavailable in this browser.')
  const request = indexedDB.open('aiide-sprite-workshop', 1)
  request.onupgradeneeded = () => { if (!request.result.objectStoreNames.contains('projects')) request.result.createObjectStore('projects', { keyPath: 'id' }) }
  return requestResult(request)
}

export async function saveLatestProject(record: StoredProject): Promise<void> {
  const database = await openDatabase()
  try { const transaction = database.transaction('projects', 'readwrite'); transaction.objectStore('projects').put(record); await transactionDone(transaction) }
  finally { database.close() }
}

export async function loadLatestProject(): Promise<StoredProject | null> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction('projects', 'readonly')
    const value = await requestResult(transaction.objectStore('projects').get('latest')) as StoredProject | undefined
    await transactionDone(transaction)
    if (!value) return null
    if (value.schemaVersion !== PROJECT_SCHEMA_VERSION) throw new Error(`The saved project uses unsupported schema version ${String(value.schemaVersion)}. It has been left intact.`)
    validateProject(value.project)
    if (!value.source?.blob || !(value.source.blob instanceof Blob)) throw new Error('The saved source PNG is missing or malformed.')
    return value
  } finally { database.close() }
}

async function blobToDataUrl(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer())
  let binary = ''
  for (let start = 0; start < bytes.length; start += 32_768) binary += String.fromCharCode(...bytes.subarray(start, start + 32_768))
  return `data:${blob.type || 'image/png'};base64,${btoa(binary)}`
}

function dataUrlToBlob(value: string): Blob {
  const match = /^data:(image\/png);base64,([A-Za-z0-9+/=]+)$/.exec(value)
  if (!match) throw new Error('Portable project source PNG is malformed.')
  let binary: string
  try { binary = atob(match[2]) } catch { throw new Error('Portable project source PNG is malformed.') }
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index)
  return new Blob([bytes], { type: match[1] })
}

export async function serializePortableProject(record: StoredProject): Promise<string> {
  const dataUrl = await blobToDataUrl(record.source.blob)
  const portable: PortableProject = { kind: PORTABLE_PROJECT_KIND, schemaVersion: PROJECT_SCHEMA_VERSION, exportedAt: new Date().toISOString(), project: validateProject(record.project), source: { name: record.source.name, type: 'image/png', size: record.source.size, lastModified: record.source.lastModified, dataUrl } }
  return JSON.stringify(portable)
}

export function deserializePortableProject(text: string): StoredProject {
  let parsed: unknown
  try { parsed = JSON.parse(text) } catch { throw new Error('Choose a valid Sprite Workshop project file.') }
  if (!parsed || typeof parsed !== 'object') throw new Error('Project file is malformed.')
  const portable = parsed as Partial<PortableProject>
  if (portable.kind !== PORTABLE_PROJECT_KIND) throw new Error('This is not a Sprite Workshop project file.')
  if (portable.schemaVersion !== PROJECT_SCHEMA_VERSION) throw new Error(`Unsupported project schema version: ${String(portable.schemaVersion)}. The current session was not changed.`)
  if (!portable.source || typeof portable.source.name !== 'string' || typeof portable.source.dataUrl !== 'string') throw new Error('Project source metadata is malformed.')
  const blob = dataUrlToBlob(portable.source.dataUrl)
  if (blob.size !== portable.source.size) throw new Error('Project source PNG is incomplete or corrupted.')
  return { id: 'latest', schemaVersion: PROJECT_SCHEMA_VERSION, savedAt: Date.now(), project: validateProject(portable.project), source: { blob, name: portable.source.name, type: 'image/png', size: blob.size, lastModified: isNumber(portable.source.lastModified) ? portable.source.lastModified : 0 } }
}

export async function restoreSource(record: SourceRecord): Promise<RestoredSource> {
  const file = new File([record.blob], record.name, { type: 'image/png', lastModified: record.lastModified })
  const source = await loadPng(file)
  const canvas = document.createElement('canvas'); canvas.width = source.width; canvas.height = source.height
  const context = canvas.getContext('2d', { willReadFrequently: true })
  if (!context) { source.bitmap.close(); throw new Error('Canvas is unavailable for source restoration.') }
  context.drawImage(source.bitmap, 0, 0)
  const imageData = context.getImageData(0, 0, source.width, source.height)
  return { source, pixels: { data: imageData.data, width: source.width, height: source.height }, blob: record.blob, metadata: { name: record.name, type: 'image/png', size: record.blob.size, lastModified: record.lastModified } }
}

export function projectFileName(name: string): string { return `${(name || 'sprite-project').replace(/[^a-z0-9_-]+/gi, '-').replace(/^-|-$/g, '') || 'sprite-project'}.spriteworkshop.json` }

export async function saveWithStatus(save: () => Promise<void>, status: (value: 'saving' | 'saved' | 'failed') => void): Promise<void> {
  status('saving')
  try { await save(); status('saved') }
  catch (cause) { status('failed'); throw cause }
}

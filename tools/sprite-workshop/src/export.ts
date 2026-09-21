import type { Region } from './geometry'
import type { Layout } from './alignment'
import { drawFrame, drawPlacement } from './render'
import type { TouchUps } from './touchups'

export function cropToCanvas(image: ImageBitmap, region: Region, touchUps: TouchUps = {}): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = region.width
  canvas.height = region.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas is unavailable in this browser.')
  drawFrame(context, image, region, touchUps[region.id] ?? [], 0, 0)
  return canvas
}

export function pngBlob(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('PNG export failed.')), 'image/png'))
}

export function alignedCanvas(image: ImageBitmap, layout: Layout, slot: number, touchUps: TouchUps = {}): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = layout.width; canvas.height = layout.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas is unavailable in this browser.')
  drawPlacement(context, image, layout.placements[slot] ?? null, touchUps)
  return canvas
}

// Schema v1: zero-based, pixel-space rectangles in playback order; durationMs is 1000 / fps.
export type AnimationMetadata = {
  schemaVersion: 1
  name: string
  frameCount: number
  fps: number
  frameDurationMs: number
  spriteSheet: string
  sheet: { width: number; height: number }
  cell: { width: number; height: number }
  layout: 'horizontal'
  frames: { slot: number; rect: { x: number; y: number; width: number; height: number } }[]
}

export function sanitizeFilename(name: string): string {
  return name.trim().replace(/[^a-z0-9_-]+/gi, '-').replace(/^-|-$/g, '').slice(0, 80).replace(/-$/g, '') || 'animation'
}

export function animationMetadata(name: string, fps: number, layout: Layout): AnimationMetadata {
  const base = sanitizeFilename(name)
  return {
    schemaVersion: 1, name: name.trim(), frameCount: 8, fps, frameDurationMs: 1000 / fps,
    spriteSheet: `${base}.png`, sheet: { width: layout.width * 8, height: layout.height },
    cell: { width: layout.width, height: layout.height }, layout: 'horizontal',
    frames: Array.from({ length: 8 }, (_, slot) => ({ slot: slot + 1, rect: { x: slot * layout.width, y: 0, width: layout.width, height: layout.height } })),
  }
}

export function validateAnimation(slots: (string | null)[], regions: Region[], layout: Layout, name: string, fps: number): string | null {
  if (!name.trim()) return 'Enter an animation name before exporting.'
  if (!Number.isInteger(fps) || fps < 1 || fps > 24) return 'FPS must be between 1 and 24.'
  if (slots.length !== 8 || layout.placements.length !== 8) return 'Animation must have exactly eight slots.'
  for (let slot = 0; slot < 8; slot++) {
    if (!slots[slot]) return `Slot ${slot + 1} is empty. Assign a frame before exporting.`
    const placement = layout.placements[slot]
    if (!regions.some(region => region.id === slots[slot]) || !placement) return `Slot ${slot + 1} references a deleted frame. Assign a frame before exporting.`
    const { x, y, source } = placement
    if (x < 0 || y < 0 || x + source.width > layout.width || y + source.height > layout.height)
      return `Slot ${slot + 1} extends outside the shared canvas. Increase padding or minimum canvas size, or adjust its offset.`
  }
  if (layout.width * 8 > 16384 || layout.height > 16384 || layout.width * 8 * layout.height > 64_000_000)
    return 'Sprite sheet exceeds the 16,384 px / 64 megapixel export limit. Reduce the shared canvas size.'
  return null
}

export function spriteSheetCanvas(image: ImageBitmap, layout: Layout, touchUps: TouchUps = {}): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = layout.width * 8; canvas.height = layout.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas is unavailable in this browser.')
  for (let slot = 0; slot < 8; slot++) {
    const placement = layout.placements[slot]
    if (!placement) throw new Error(`Slot ${slot + 1} has no placement.`)
    drawPlacement(context, image, { ...placement, x: placement.x + slot * layout.width }, touchUps)
  }
  return canvas
}

function triggerDownload(url: string, filename: string): void {
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  document.body.append(link)
  try { link.click() }
  finally { link.remove() }
}

export async function downloadAnimation(image: ImageBitmap, slots: (string | null)[], regions: Region[], layout: Layout, name: string, fps: number, touchUps: TouchUps = {}): Promise<void> {
  const issue = validateAnimation(slots, regions, layout, name, fps)
  if (issue) throw new Error(issue)
  const metadata = animationMetadata(name, fps, layout)
  const png = await pngBlob(spriteSheetCanvas(image, layout, touchUps))
  // Prepare both files before triggering either download.
  const json = new Blob([JSON.stringify(metadata, null, 2) + '\n'], { type: 'application/json' })
  const pngUrl = URL.createObjectURL(png)
  let jsonUrl: string
  try { jsonUrl = URL.createObjectURL(json) }
  catch (cause) { URL.revokeObjectURL(pngUrl); throw cause }
  try {
    triggerDownload(pngUrl, metadata.spriteSheet)
    triggerDownload(jsonUrl, `${sanitizeFilename(name)}.json`)
  } finally {
    window.setTimeout(() => { URL.revokeObjectURL(pngUrl); URL.revokeObjectURL(jsonUrl) }, 30_000)
  }
}

export async function downloadRegion(image: ImageBitmap, region: Region, sourceName: string, touchUps: TouchUps = {}): Promise<void> {
  const blob = await pngBlob(cropToCanvas(image, region, touchUps))
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  const base = sourceName.replace(/\.png$/i, '').replace(/[^a-z0-9_-]+/gi, '-').replace(/^-|-$/g, '') || 'sprite'
  const name = region.name.replace(/[^a-z0-9_-]+/gi, '-').replace(/^-|-$/g, '') || 'frame'
  link.href = url
  link.download = `${base}-${name}-${region.id.slice(0, 6)}.png`
  document.body.append(link)
  link.click()
  link.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 30_000)
}

export async function downloadAlignedSlot(image: ImageBitmap, layout: Layout, slot: number, sourceName: string, touchUps: TouchUps = {}): Promise<void> {
  const blob = await pngBlob(alignedCanvas(image, layout, slot, touchUps))
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  const base = sourceName.replace(/\.png$/i, '').replace(/[^a-z0-9_-]+/gi, '-').replace(/^-|-$/g, '') || 'sprite'
  link.href = url
  link.download = `${base}-animation-${String(slot + 1).padStart(2, '0')}.png`
  document.body.append(link)
  link.click()
  link.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 30_000)
}

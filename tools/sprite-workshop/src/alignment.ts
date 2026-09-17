import type { Region } from './geometry'

export type Pixels = { data: Uint8ClampedArray; width: number; height: number }
export type Bounds = { x: number; y: number; width: number; height: number }
export type AlignmentMode = 'bottom' | 'center' | 'original'
export type AlignmentOptions = { mode: AlignmentMode; padding: number; minWidth: number; minHeight: number }
export type Placement = { source: Bounds; x: number; y: number; anchorX: number; anchorY: number }
export type Layout = { width: number; height: number; anchorX: number; anchorY: number; placements: Record<string, Placement | null> }

export function visibleBounds(pixels: Pixels, region: Region): Bounds | null {
  const left = Math.max(0, region.x), top = Math.max(0, region.y)
  const right = Math.min(pixels.width, region.x + region.width), bottom = Math.min(pixels.height, region.y + region.height)
  let minX = right, minY = bottom, maxX = left - 1, maxY = top - 1
  for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
    if (pixels.data[(y * pixels.width + x) * 4 + 3] === 0) continue
    minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x); maxY = Math.max(maxY, y)
  }
  return maxX < minX ? null : { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 }
}

export function calculateLayout(pixels: Pixels, regions: Region[], options: AlignmentOptions): Layout {
  const padding = Math.max(0, Math.round(options.padding))
  const entries = regions.map(region => {
    const source = options.mode === 'original'
      ? { x: region.x, y: region.y, width: region.width, height: region.height }
      : visibleBounds(pixels, region)
    if (!source) return { id: region.id, source: null, left: 0, top: 0 }
    const offsetX = Math.round(region.anchorX ?? 0), offsetY = Math.round(region.anchorY ?? 0)
    const left = (options.mode === 'original' ? 0 : -Math.floor(source.width / 2)) + offsetX
    const top = (options.mode === 'bottom' ? -source.height : options.mode === 'center' ? -Math.floor(source.height / 2) : 0) + offsetY
    return { id: region.id, source, left, top }
  })
  const visible = entries.filter(entry => entry.source !== null)
  if (!visible.length) return { width: Math.max(1, options.minWidth, padding * 2 + 1), height: Math.max(1, options.minHeight, padding * 2 + 1), anchorX: padding, anchorY: padding, placements: Object.fromEntries(entries.map(entry => [entry.id, null])) }
  const minX = Math.min(0, ...visible.map(entry => entry.left))
  const minY = Math.min(0, ...visible.map(entry => entry.top))
  const maxX = Math.max(0, ...visible.map(entry => entry.left + entry.source!.width))
  const maxY = Math.max(0, ...visible.map(entry => entry.top + entry.source!.height))
  const contentWidth = maxX - minX, contentHeight = maxY - minY
  const width = Math.max(contentWidth + padding * 2, Math.round(options.minWidth) || 0)
  const height = Math.max(contentHeight + padding * 2, Math.round(options.minHeight) || 0)
  const shiftX = padding + Math.floor((width - contentWidth - padding * 2) / 2) - minX
  const shiftY = padding + Math.floor((height - contentHeight - padding * 2) / 2) - minY
  const anchorX = shiftX, anchorY = shiftY
  return { width, height, anchorX, anchorY, placements: Object.fromEntries(entries.map(entry => [entry.id, entry.source ? { source: entry.source, x: entry.left + shiftX, y: entry.top + shiftY, anchorX, anchorY } : null])) }
}

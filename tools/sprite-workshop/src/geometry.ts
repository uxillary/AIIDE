export type Region = { id: string; name: string; x: number; y: number; width: number; height: number }
export type Point = { x: number; y: number }
export type Size = { width: number; height: number }
export type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w'

export function clamp(value: number, min: number, max: number): number { return Math.min(max, Math.max(min, value)) }

export function screenToImage(point: Point, pan: Point, zoom: number, size: Size): Point {
  return {
    x: clamp(Math.floor((point.x - pan.x) / zoom), 0, size.width - 1),
    y: clamp(Math.floor((point.y - pan.y) / zoom), 0, size.height - 1),
  }
}

export function zoomAt(pan: Point, point: Point, oldZoom: number, nextZoom: number): Point {
  return { x: point.x - (point.x - pan.x) * nextZoom / oldZoom, y: point.y - (point.y - pan.y) * nextZoom / oldZoom }
}

export function fitZoom(image: Size, viewport: Size): number {
  return clamp(Math.min((viewport.width - 64) / image.width, (viewport.height - 64) / image.height), 0.25, 8)
}

export function boundedRegion(region: Region, size: Size): Region {
  const x = clamp(Math.round(region.x), 0, size.width - 1)
  const y = clamp(Math.round(region.y), 0, size.height - 1)
  return { ...region, x, y, width: clamp(Math.round(region.width), 1, size.width - x), height: clamp(Math.round(region.height), 1, size.height - y) }
}

export function regionFromCorners(a: Point, b: Point, id: string, name: string): Region {
  return { id, name, x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(a.x - b.x) + 1, height: Math.abs(a.y - b.y) + 1 }
}

export function resizeRegion(region: Region, handle: Handle, dx: number, dy: number, size: Size): Region {
  let left = region.x, top = region.y, right = region.x + region.width, bottom = region.y + region.height
  if (handle.includes('w')) left = clamp(region.x + dx, 0, right - 1)
  if (handle.includes('e')) right = clamp(region.x + region.width + dx, left + 1, size.width)
  if (handle.includes('n')) top = clamp(region.y + dy, 0, bottom - 1)
  if (handle.includes('s')) bottom = clamp(region.y + region.height + dy, top + 1, size.height)
  return { ...region, x: left, y: top, width: right - left, height: bottom - top }
}

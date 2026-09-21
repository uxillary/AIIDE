import type { Point, Region, Size } from './geometry'

export type EraseRect = { x: number; y: number; width: number; height: number }
export type TouchUps = Record<string, EraseRect[]>
export type TouchUpHistoryStep = { touchUps: TouchUps; from: TouchUps[]; to: TouchUps[] }

export const MAX_ERASE_RECTS_PER_FRAME = 512

export function eraseRectFromPoints(a: Point, b: Point, size: Size): EraseRect {
  const left = Math.max(0, Math.min(size.width - 1, Math.min(Math.floor(a.x), Math.floor(b.x))))
  const top = Math.max(0, Math.min(size.height - 1, Math.min(Math.floor(a.y), Math.floor(b.y))))
  const right = Math.max(left, Math.min(size.width - 1, Math.max(Math.floor(a.x), Math.floor(b.x))))
  const bottom = Math.max(top, Math.min(size.height - 1, Math.max(Math.floor(a.y), Math.floor(b.y))))
  return { x: left, y: top, width: right - left + 1, height: bottom - top + 1 }
}

export function framePoint(client: Point, bounds: { left: number; top: number; width: number; height: number }, size: Size): Point {
  return {
    x: Math.max(0, Math.min(size.width - 1, Math.floor((client.x - bounds.left) * size.width / Math.max(1, bounds.width)))),
    y: Math.max(0, Math.min(size.height - 1, Math.floor((client.y - bounds.top) * size.height / Math.max(1, bounds.height)))),
  }
}

export function appendErase(touchUps: TouchUps, frameId: string, rect: EraseRect): TouchUps {
  const current = touchUps[frameId] ?? []
  if (current.length >= MAX_ERASE_RECTS_PER_FRAME) return touchUps
  return { ...touchUps, [frameId]: [...current, rect] }
}

export function resetFrameTouchUps(touchUps: TouchUps, frameId: string): TouchUps {
  if (!(frameId in touchUps)) return touchUps
  const next = { ...touchUps }
  delete next[frameId]
  return next
}

export function stepTouchUpHistory(from: TouchUps[], to: TouchUps[], current: TouchUps): TouchUpHistoryStep | null {
  const touchUps = from.at(-1)
  if (!touchUps) return null
  return { touchUps, from: from.slice(0, -1), to: [...to.slice(-99), current] }
}

function isValidRect(value: unknown, frame: Region): value is EraseRect {
  if (!value || typeof value !== 'object') return false
  const rect = value as EraseRect
  return [rect.x, rect.y, rect.width, rect.height].every(Number.isInteger)
    && rect.x >= 0 && rect.y >= 0 && rect.width > 0 && rect.height > 0
    && rect.x + rect.width <= frame.width && rect.y + rect.height <= frame.height
}

export function validateTouchUps(value: unknown, regions: Region[]): TouchUps {
  if (value === undefined) return {}
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Project touch-ups are malformed.')
  const frames = new Map(regions.map(frame => [frame.id, frame]))
  const result: TouchUps = {}
  for (const [frameId, rectangles] of Object.entries(value)) {
    const frame = frames.get(frameId)
    if (!frame || !Array.isArray(rectangles) || rectangles.length > MAX_ERASE_RECTS_PER_FRAME || !rectangles.every(rect => isValidRect(rect, frame)))
      throw new Error('Project touch-ups are malformed or outside their frame bounds.')
    if (rectangles.length) result[frameId] = rectangles.map(rect => ({ ...rect }))
  }
  return result
}

import type { Point, Region } from './geometry'

export type Pixels = { data: Uint8ClampedArray; width: number; height: number }
export type Bounds = { x: number; y: number; width: number; height: number }
export type AlignmentMode = 'bottom' | 'center'
export type AlignmentOptions = { mode: AlignmentMode; padding: number; minWidth: number; minHeight: number }
export type Placement = { source: Bounds; x: number; y: number }
export type Layout = { width: number; height: number; anchorX: number; anchorY: number; placements: (Placement | null)[] }

export const emptyOffsets = (): Point[] => Array.from({ length: 8 }, () => ({ x: 0, y: 0 }))

// Size from source crops, never offsets or alpha bounds, so nudging cannot move the canvas origin.
export function calculateLayout(frames: (Region | null)[], offsets: Point[], options: AlignmentOptions): Layout {
  const padding = Math.max(0, Math.round(options.padding))
  const width = Math.max(1, Math.round(options.minWidth), ...frames.map(frame => (frame?.width ?? 0) + padding * 2))
  const height = Math.max(1, Math.round(options.minHeight), ...frames.map(frame => (frame?.height ?? 0) + padding * 2))
  const anchorX = Math.floor(width / 2)
  const anchorY = options.mode === 'bottom' ? height - padding : Math.floor(height / 2)
  const placements = frames.map((frame, index): Placement | null => {
    if (!frame) return null
    const offset = offsets[index] ?? { x: 0, y: 0 }
    return {
      source: { x: frame.x, y: frame.y, width: frame.width, height: frame.height },
      x: anchorX - Math.floor(frame.width / 2) + Math.round(offset.x),
      y: (options.mode === 'bottom' ? anchorY - frame.height : anchorY - Math.floor(frame.height / 2)) + Math.round(offset.y),
    }
  })
  return { width, height, anchorX, anchorY, placements }
}

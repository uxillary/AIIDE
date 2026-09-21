import type { Placement } from './alignment'
import type { Region } from './geometry'
import type { EraseRect, TouchUps } from './touchups'

export function drawFrame(context: CanvasRenderingContext2D, image: ImageBitmap, frame: Region | Placement['source'], erases: EraseRect[], x: number, y: number, width = frame.width, height = frame.height): void {
  context.imageSmoothingEnabled = false
  if (!erases.length) {
    context.drawImage(image, frame.x, frame.y, frame.width, frame.height, x, y, width, height)
    return
  }
  const composite = document.createElement('canvas')
  composite.width = frame.width
  composite.height = frame.height
  const compositeContext = composite.getContext('2d')
  if (!compositeContext) throw new Error('Canvas is unavailable in this browser.')
  compositeContext.imageSmoothingEnabled = false
  compositeContext.drawImage(image, frame.x, frame.y, frame.width, frame.height, 0, 0, frame.width, frame.height)
  for (const rect of erases) compositeContext.clearRect(rect.x, rect.y, rect.width, rect.height)
  context.drawImage(composite, 0, 0, frame.width, frame.height, x, y, width, height)
}

export function drawPlacement(context: CanvasRenderingContext2D, image: ImageBitmap, placement: Placement | null, touchUps: TouchUps = {}): void {
  if (!placement) return
  const { source, x, y } = placement
  drawFrame(context, image, source, placement.frameId ? touchUps[placement.frameId] ?? [] : [], x, y)
}

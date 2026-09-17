import type { Placement } from './alignment'

export function drawPlacement(context: CanvasRenderingContext2D, image: ImageBitmap, placement: Placement | null): void {
  if (!placement) return
  const { source, x, y } = placement
  context.imageSmoothingEnabled = false
  context.drawImage(image, source.x, source.y, source.width, source.height, x, y, source.width, source.height)
}

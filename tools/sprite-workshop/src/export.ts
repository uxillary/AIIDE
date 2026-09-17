import type { Region } from './geometry'
import type { Layout } from './alignment'
import { drawPlacement } from './render'

export function cropToCanvas(image: ImageBitmap, region: Region): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = region.width
  canvas.height = region.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas is unavailable in this browser.')
  context.imageSmoothingEnabled = false
  context.drawImage(image, region.x, region.y, region.width, region.height, 0, 0, region.width, region.height)
  return canvas
}

export function pngBlob(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('PNG export failed.')), 'image/png'))
}

export function alignedCanvas(image: ImageBitmap, layout: Layout, id: string): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = layout.width; canvas.height = layout.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas is unavailable in this browser.')
  drawPlacement(context, image, layout.placements[id] ?? null)
  return canvas
}

export async function downloadRegion(image: ImageBitmap, region: Region, sourceName: string, layout?: Layout): Promise<void> {
  const blob = await pngBlob(layout ? alignedCanvas(image, layout, region.id) : cropToCanvas(image, region))
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

import { memo, useEffect, useRef } from 'react'
import type { Region } from './geometry'
import type { Placement } from './alignment'
import { drawPlacement } from './render'

export function SpriteCanvas({ image, region, placement, onionPlacement, width, height, className = '' }: { image: ImageBitmap; region?: Region; placement?: Placement | null; onionPlacement?: Placement | null; width: number; height: number; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    context.clearRect(0, 0, width, height)
    context.imageSmoothingEnabled = false
    if (onionPlacement) {
      context.globalAlpha = 0.35
      drawPlacement(context, image, onionPlacement)
      context.globalAlpha = 1
    }
    if (placement !== undefined) drawPlacement(context, image, placement)
    else if (region) context.drawImage(image, region.x, region.y, region.width, region.height, Math.floor((width - region.width) / 2), Math.floor((height - region.height) / 2), region.width, region.height)
  }, [image, region, placement, onionPlacement, width, height])
  return <canvas ref={ref} width={width} height={height} className={className} />
}

export const Thumbnail = memo(function Thumbnail({ image, region }: { image: ImageBitmap; region: Region }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    context.clearRect(0, 0, canvas.width, canvas.height)
    context.imageSmoothingEnabled = false
    const scale = Math.min(canvas.width / region.width, canvas.height / region.height)
    const width = Math.max(1, Math.floor(region.width * scale))
    const height = Math.max(1, Math.floor(region.height * scale))
    context.drawImage(image, region.x, region.y, region.width, region.height, Math.floor((canvas.width - width) / 2), Math.floor((canvas.height - height) / 2), width, height)
  }, [image, region])
  return <canvas ref={ref} width={52} height={52} className="thumbnail checker" aria-hidden="true" />
})

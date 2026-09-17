import { useEffect, useRef } from 'react'
import type { Region } from './geometry'

export function SpriteCanvas({ image, region, width, height, className = '' }: { image: ImageBitmap; region?: Region; width: number; height: number; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    context.clearRect(0, 0, width, height)
    context.imageSmoothingEnabled = false
    if (region) context.drawImage(image, region.x, region.y, region.width, region.height, Math.floor((width - region.width) / 2), Math.floor((height - region.height) / 2), region.width, region.height)
  }, [image, region, width, height])
  return <canvas ref={ref} width={width} height={height} className={className} />
}

export function Thumbnail({ image, region }: { image: ImageBitmap; region: Region }) {
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
}

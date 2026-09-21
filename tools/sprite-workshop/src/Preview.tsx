import { memo, useEffect, useRef, useState } from 'react'
import type { Region } from './geometry'
import type { Placement } from './alignment'
import { drawFrame, drawPlacement } from './render'
import { eraseRectFromPoints, framePoint } from './touchups'
import type { EraseRect, TouchUps } from './touchups'

export function SpriteCanvas({ image, region, placement, onionPlacement, width, height, touchUps = {}, className = '' }: { image: ImageBitmap; region?: Region; placement?: Placement | null; onionPlacement?: Placement | null; width: number; height: number; touchUps?: TouchUps; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    context.clearRect(0, 0, width, height)
    context.imageSmoothingEnabled = false
    if (onionPlacement) {
      context.globalAlpha = 0.35
      drawPlacement(context, image, onionPlacement, touchUps)
      context.globalAlpha = 1
    }
    if (placement !== undefined) drawPlacement(context, image, placement, touchUps)
    else if (region) drawFrame(context, image, region, touchUps[region.id] ?? [], Math.floor((width - region.width) / 2), Math.floor((height - region.height) / 2))
  }, [image, region, placement, onionPlacement, width, height, touchUps])
  return <canvas ref={ref} width={width} height={height} className={className} />
}

export const Thumbnail = memo(function Thumbnail({ image, region, touchUps = {} }: { image: ImageBitmap; region: Region; touchUps?: TouchUps }) {
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
    drawFrame(context, image, region, touchUps[region.id] ?? [], Math.floor((canvas.width - width) / 2), Math.floor((canvas.height - height) / 2), width, height)
  }, [image, region, touchUps])
  return <canvas ref={ref} width={52} height={52} className="thumbnail checker" aria-hidden="true" />
})

export function TouchUpEditor({ image, region, erases, active, zoom, onErase }: { image: ImageBitmap; region: Region; erases: EraseRect[]; active: boolean; zoom: number; onErase: (rect: EraseRect) => void }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const drag = useRef<{ start: { x: number; y: number }; pointerId: number } | null>(null)
  const [draft, setDraft] = useState<EraseRect | null>(null)
  const [cursor, setCursor] = useState({ x: 0, y: 0 })
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null)

  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    context.clearRect(0, 0, region.width, region.height)
    drawFrame(context, image, region, erases, 0, 0)
    const feedback = draft ?? (active ? eraseRectFromPoints(anchor ?? cursor, cursor, region) : null)
    if (feedback) {
      context.save()
      context.fillStyle = '#ff6f6166'
      context.strokeStyle = '#fff0a6'
      context.lineWidth = 1
      context.fillRect(feedback.x, feedback.y, feedback.width, feedback.height)
      context.strokeRect(feedback.x + 0.5, feedback.y + 0.5, Math.max(0, feedback.width - 1), Math.max(0, feedback.height - 1))
      context.restore()
    }
  }, [image, region, erases, active, draft, cursor, anchor])

  function point(event: React.PointerEvent<HTMLCanvasElement>) {
    const bounds = event.currentTarget.getBoundingClientRect()
    return framePoint({ x: event.clientX, y: event.clientY }, bounds, region)
  }

  return <canvas ref={ref} width={region.width} height={region.height} style={{ width: region.width * zoom, height: region.height * zoom }} className={active ? 'touchup-canvas active' : 'touchup-canvas'} tabIndex={active ? 0 : -1} role="img" aria-label={active ? 'Rectangle erase editor. Drag to erase. Arrow keys move the pixel cursor, Shift plus arrows sizes a rectangle, and Enter erases it.' : `Edited preview of ${region.name}`}
    onPointerDown={event => { if (!active || event.button !== 0) return; event.preventDefault(); const start = point(event); drag.current = { start, pointerId: event.pointerId }; setCursor(start); setAnchor(start); setDraft(eraseRectFromPoints(start, start, region)); event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.focus() }}
    onPointerMove={event => { if (!drag.current) return; const current = point(event); setCursor(current); setDraft(eraseRectFromPoints(drag.current.start, current, region)) }}
    onPointerUp={event => { if (!drag.current) return; const rect = eraseRectFromPoints(drag.current.start, point(event), region); drag.current = null; setDraft(null); setAnchor(null); onErase(rect); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId) }}
    onPointerCancel={() => { drag.current = null; setDraft(null); setAnchor(null) }}
    onKeyDown={event => { if (!active) return; const delta = event.key === 'ArrowLeft' ? { x: -1, y: 0 } : event.key === 'ArrowRight' ? { x: 1, y: 0 } : event.key === 'ArrowUp' ? { x: 0, y: -1 } : event.key === 'ArrowDown' ? { x: 0, y: 1 } : null; if (delta) { event.preventDefault(); const next = { x: Math.max(0, Math.min(region.width - 1, cursor.x + delta.x)), y: Math.max(0, Math.min(region.height - 1, cursor.y + delta.y)) }; setAnchor(event.shiftKey ? anchor ?? cursor : null); setCursor(next) } else if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onErase(eraseRectFromPoints(anchor ?? cursor, cursor, region)); setAnchor(null) } else if (event.key === 'Escape') { setAnchor(null) } }} />
}

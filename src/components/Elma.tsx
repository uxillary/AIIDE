import { useEffect, useRef, type CSSProperties } from 'react'
import idleSprite from '../assets/elma/runtime/elma-idle.png'
import thinkingSprite from '../assets/elma/runtime/elma-thinking.png'
import workingSprite from '../assets/elma/runtime/elma-working.png'
import inspectingSprite from '../assets/elma/runtime/elma-inspecting.png'
import successSprite from '../assets/elma/runtime/elma-success.png'
import errorSprite from '../assets/elma/runtime/elma-error.png'
import portalInSprite from '../assets/elma/elma-portal-jump-in/elma-portal-jump-in.png'
import portalInMetadata from '../assets/elma/elma-portal-jump-in/elma-portal-jump-in.json'
import portalOutSprite from '../assets/elma/elma-portal-jump-out/elma-portal-jump-out.png'
import portalOutMetadata from '../assets/elma/elma-portal-jump-out/elma-portal-jump-out.json'
import preparingChangesSprite from '../assets/elma/preparing-changes/preparing-changes.png'
import preparingChangesMetadata from '../assets/elma/preparing-changes/preparing-changes.json'
import changesReadySprite from '../assets/elma/changes-ready/changes-ready.png'
import changesReadyMetadata from '../assets/elma/changes-ready/changes-ready.json'
import changesAppliedSprite from '../assets/elma/changes-applied/changes-applied.png'
import changesAppliedMetadata from '../assets/elma/changes-applied/changes-applied.json'
import imageThinkingSprite from '../assets/elma/elma-image-thinking/elma-image-thinking.png'
import imageThinkingMetadata from '../assets/elma/elma-image-thinking/elma-image-thinking.json'
import imageCreatingSprite from '../assets/elma/elma-image-creating/elma-image-creating.png'
import imageCreatingMetadata from '../assets/elma/elma-image-creating/elma-image-creating.json'
import imageCompleteSprite from '../assets/elma/elma-image-complete/elma-image-complete.png'
import imageCompleteMetadata from '../assets/elma/elma-image-complete/elma-image-complete.json'

export type ElmaState = 'idle' | 'thinking' | 'working' | 'inspecting' | 'success' | 'error' | 'image-thinking' | 'image-creating' | 'image-complete'
export type ElmaAnimation = ElmaState | 'portal-in' | 'portal-out' | 'preparing-changes' | 'changes-ready' | 'changes-applied'
export type ElmaLocation = 'chat' | 'changes'

export interface ElmaPresentation {
  state: ElmaState
  animation?: ElmaAnimation
  status: string
  activity?: string
}

interface SpriteMetadata {
  frameCount: number
  fps: number
  sheet: { width: number; height: number }
  cell: { width: number; height: number }
}

interface AnimationDefinition {
  sprite: string
  frameCount: number
  fps: number
  sheetWidth: number
  sheetHeight: number
  cellWidth: number
  cellHeight: number
  footprintScale: number
  once?: boolean
}

const runtimeAnimation = (sprite: string, durationSeconds: number): AnimationDefinition => ({
  sprite,
  frameCount: 8,
  fps: 8 / durationSeconds,
  sheetWidth: 800,
  sheetHeight: 100,
  cellWidth: 100,
  cellHeight: 100,
  footprintScale: 1,
})

const metadataAnimation = (sprite: string, metadata: SpriteMetadata, once = false): AnimationDefinition => ({
  sprite,
  frameCount: metadata.frameCount,
  fps: metadata.fps,
  sheetWidth: metadata.sheet.width,
  sheetHeight: metadata.sheet.height,
  cellWidth: metadata.cell.width,
  cellHeight: metadata.cell.height,
  footprintScale: 1.33,
  once,
})

const animations: Record<ElmaAnimation, AnimationDefinition> = {
  idle: runtimeAnimation(idleSprite, 2.4),
  thinking: runtimeAnimation(thinkingSprite, 1.6),
  working: runtimeAnimation(workingSprite, 1.6),
  inspecting: runtimeAnimation(inspectingSprite, 1.6),
  success: runtimeAnimation(successSprite, 1.2),
  error: runtimeAnimation(errorSprite, 1.6),
  'portal-in': metadataAnimation(portalInSprite, portalInMetadata, true),
  'portal-out': metadataAnimation(portalOutSprite, portalOutMetadata, true),
  'preparing-changes': metadataAnimation(preparingChangesSprite, preparingChangesMetadata),
  'changes-ready': metadataAnimation(changesReadySprite, changesReadyMetadata),
  'changes-applied': metadataAnimation(changesAppliedSprite, changesAppliedMetadata),
  'image-thinking': metadataAnimation(imageThinkingSprite, imageThinkingMetadata),
  'image-creating': metadataAnimation(imageCreatingSprite, imageCreatingMetadata),
  'image-complete': metadataAnimation(imageCompleteSprite, imageCompleteMetadata),
}

type ElmaStyle = CSSProperties & Record<`--elma-${string}`, string | number>

export function Elma({ state, animation = state, size = 'compact', className = '', animated = true, onAnimationComplete }: { state: ElmaState; animation?: ElmaAnimation; size?: 'tiny' | 'compact' | 'presence' | 'hero'; className?: string; animated?: boolean; onAnimationComplete?: () => void }) {
  const definition = animations[animation]
  const completedRef = useRef(false)
  const maxDimension = Math.max(definition.cellWidth, definition.cellHeight)
  const frameWidthFactor = definition.footprintScale * definition.cellWidth / maxDimension
  const frameHeightFactor = definition.footprintScale * definition.cellHeight / maxDimension
  const sheetWidthFactor = definition.footprintScale * definition.sheetWidth / maxDimension
  const sheetHeightFactor = definition.footprintScale * definition.sheetHeight / maxDimension
  const duration = definition.frameCount / definition.fps

  useEffect(() => {
    completedRef.current = false
    if (!definition.once || !onAnimationComplete || !window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const timer = window.setTimeout(onAnimationComplete, 0)
    return () => window.clearTimeout(timer)
  }, [animation, definition.once, onAnimationComplete])

  const completeOnce = () => {
    if (!definition.once || completedRef.current) return
    completedRef.current = true
    onAnimationComplete?.()
  }

  const style = {
    '--elma-footprint-scale': definition.footprintScale,
    '--elma-duration': `${duration}s`,
    '--elma-frame-count': definition.frameCount,
    '--elma-frame-width': `calc(var(--elma-size) * ${frameWidthFactor})`,
    '--elma-frame-height': `calc(var(--elma-size) * ${frameHeightFactor})`,
    '--elma-sheet-width': `calc(var(--elma-size) * ${sheetWidthFactor})`,
    '--elma-sheet-height': `calc(var(--elma-size) * ${sheetHeightFactor})`,
    '--elma-travel': `calc(var(--elma-size) * ${-sheetWidthFactor})`,
  } as ElmaStyle

  return <span
    aria-hidden="true"
    className={`elma-sprite elma-sprite-${size}${className ? ` ${className}` : ''}`}
    data-state={state}
    data-animation={animation}
    style={style}
  >
    <span
      key={animation}
      className={`elma-sprite-frame${animated ? '' : ' elma-sprite-static'}${definition.once ? ' elma-sprite-once' : ''}`}
      style={{ backgroundImage: `url(${definition.sprite})` }}
      onAnimationEnd={completeOnce}
    />
  </span>
}

export function ElmaPresence({ presentation, animation, onAnimationComplete, className = '' }: { presentation: ElmaPresentation; animation?: ElmaAnimation; onAnimationComplete?: () => void; className?: string }) {
  return <div className={`elma-status elma-primary${className ? ` ${className}` : ''}`} data-elma-primary="true">
    <Elma state={presentation.state} animation={animation ?? presentation.animation} size="presence" onAnimationComplete={onAnimationComplete} />
    <div>
      <strong>{presentation.status}</strong>
      {presentation.activity && <span>{presentation.activity}</span>}
    </div>
  </div>
}

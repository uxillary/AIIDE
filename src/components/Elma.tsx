import type { CSSProperties } from 'react'
import idleSprite from '../assets/elma/runtime/elma-idle.png'
import thinkingSprite from '../assets/elma/runtime/elma-thinking.png'
import workingSprite from '../assets/elma/runtime/elma-working.png'
import inspectingSprite from '../assets/elma/runtime/elma-inspecting.png'
import successSprite from '../assets/elma/runtime/elma-success.png'
import errorSprite from '../assets/elma/runtime/elma-error.png'

export type ElmaState = 'idle' | 'thinking' | 'working' | 'inspecting' | 'success' | 'error'

const animation: Record<ElmaState, { sprite: string; duration: string }> = {
  idle: { sprite: idleSprite, duration: '2.4s' },
  thinking: { sprite: thinkingSprite, duration: '1.6s' },
  working: { sprite: workingSprite, duration: '1.6s' },
  inspecting: { sprite: inspectingSprite, duration: '1.6s' },
  success: { sprite: successSprite, duration: '1.2s' },
  error: { sprite: errorSprite, duration: '1.6s' },
}

type ElmaStyle = CSSProperties & { '--elma-duration': string }

export function Elma({ state }: { state: ElmaState }) {
  const { sprite, duration } = animation[state]
  return <span
    aria-hidden="true"
    className="elma-sprite"
    data-state={state}
    style={{ backgroundImage: `url(${sprite})`, '--elma-duration': duration } as ElmaStyle}
  />
}

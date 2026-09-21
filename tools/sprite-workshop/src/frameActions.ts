import type { Region } from './geometry'

export type FrameState = { regions: Region[]; slots: (string | null)[]; selectedId: string | null }
export type DeletedFrame = { region: Region; index: number; slotIndexes: number[] }

export function slotsFromFrameOrder(regions: Region[], slots: (string | null)[]): { slots: (string | null)[]; replacesAssignments: boolean } {
  const next = Array.from({ length: 8 }, (_, index) => regions[index]?.id ?? null)
  return { slots: next, replacesAssignments: slots.some((id, index) => id !== null && id !== next[index]) }
}

export function removeFrame(state: FrameState, id: string): { state: FrameState; undo: DeletedFrame | null } {
  const index = state.regions.findIndex(region => region.id === id)
  if (index < 0) return { state, undo: null }
  const region = state.regions[index]
  const regions = state.regions.filter(item => item.id !== id)
  const slotIndexes = state.slots.flatMap((slot, slotIndex) => slot === id ? [slotIndex] : [])
  return { state: { regions, slots: state.slots.map(slot => slot === id ? null : slot), selectedId: state.selectedId === id ? regions[Math.min(index, regions.length - 1)]?.id ?? null : state.selectedId }, undo: { region, index, slotIndexes } }
}

export function restoreFrame(state: FrameState, deleted: DeletedFrame): FrameState {
  if (state.regions.some(region => region.id === deleted.region.id)) return state
  const regions = [...state.regions]
  regions.splice(Math.min(deleted.index, regions.length), 0, deleted.region)
  return { regions, slots: state.slots.map((slot, index) => slot === null && deleted.slotIndexes.includes(index) ? deleted.region.id : slot), selectedId: deleted.region.id }
}

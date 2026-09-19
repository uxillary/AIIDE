import type { Pixels } from './alignment'
import type { Region, Size } from './geometry'

export type GridOptions = { rows: number; columns: number; gapX: number; gapY: number; left: number; right: number; top: number; bottom: number }
export type AlphaOptions = { joinGap: number; minPixels: number }
export const MAX_FRAME_COUNT = 256

export type SuggestionAcceptance = { regions: Region[]; suggestions: Region[]; added: number; skipped: number; rejected: number }

function sameRegion(a: Region, b: Region): boolean {
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height
}

export function validSuggestion(region: Region, size: Size): boolean {
  return [region.x, region.y, region.width, region.height].every(Number.isInteger)
    && region.x >= 0 && region.y >= 0 && region.width > 0 && region.height > 0
    && region.x + region.width <= size.width && region.y + region.height <= size.height
}

export function eligibleSuggestionCount(regions: Region[], suggestions: Region[], size: Size, maxFrames = MAX_FRAME_COUNT): number {
  const accepted = [...regions]
  let count = 0
  for (const suggestion of suggestions) {
    if (!validSuggestion(suggestion, size) || accepted.some(region => sameRegion(region, suggestion)) || accepted.length >= maxFrames) continue
    accepted.push(suggestion)
    count++
  }
  return count
}

export function acceptAllSuggestions(regions: Region[], suggestions: Region[], size: Size, createId: () => string, maxFrames = MAX_FRAME_COUNT): SuggestionAcceptance {
  const accepted = [...regions]
  let added = 0, skipped = 0, rejected = 0
  for (const suggestion of suggestions) {
    if (!validSuggestion(suggestion, size)) { rejected++; continue }
    if (accepted.some(region => sameRegion(region, suggestion))) { skipped++; continue }
    if (accepted.length >= maxFrames) { rejected++; continue }
    accepted.push({ ...suggestion, id: createId() })
    added++
  }
  return { regions: accepted, suggestions: [], added, skipped, rejected }
}

export function gridSuggestions(size: Size, options: GridOptions): Region[] {
  const rows = Math.max(1, Math.min(64, Math.round(options.rows)))
  const columns = Math.max(1, Math.min(64, Math.round(options.columns)))
  if (rows * columns > 256) return []
  const gapX = Math.max(0, Math.round(options.gapX)), gapY = Math.max(0, Math.round(options.gapY))
  const left = Math.max(0, Math.round(options.left)), right = Math.max(0, Math.round(options.right))
  const top = Math.max(0, Math.round(options.top)), bottom = Math.max(0, Math.round(options.bottom))
  const availableWidth = size.width - left - right - (columns - 1) * gapX
  const availableHeight = size.height - top - bottom - (rows - 1) * gapY
  if (availableWidth < columns || availableHeight < rows) return []
  const cellWidth = availableWidth / columns, cellHeight = availableHeight / rows
  const regions: Region[] = []
  for (let row = 0; row < rows; row++) for (let column = 0; column < columns; column++) {
    const x = Math.round(left + column * (cellWidth + gapX))
    const y = Math.round(top + row * (cellHeight + gapY))
    const endX = Math.round(left + (column + 1) * cellWidth + column * gapX)
    const endY = Math.round(top + (row + 1) * cellHeight + row * gapY)
    regions.push({ id: `suggestion-${row}-${column}`, name: `Row ${row + 1} · Frame ${column + 1}`, x, y, width: endX - x, height: endY - y })
  }
  return regions
}

type Island = { x: number; y: number; right: number; bottom: number; pixels: number }

export function alphaSuggestions(source: Pixels, options: AlphaOptions): Region[] {
  const { width, height, data } = source
  const visited = new Uint8Array(width * height)
  const islands: Island[] = []
  for (let index = 0; index < visited.length; index++) {
    if (visited[index] || data[index * 4 + 3] === 0) continue
    const queue = [index]
    visited[index] = 1
    let head = 0, x = index % width, y = Math.floor(index / width)
    const island = { x, y, right: x + 1, bottom: y + 1, pixels: 0 }
    while (head < queue.length) {
      const current = queue[head++]
      x = current % width; y = Math.floor(current / width)
      island.x = Math.min(island.x, x); island.y = Math.min(island.y, y)
      island.right = Math.max(island.right, x + 1); island.bottom = Math.max(island.bottom, y + 1); island.pixels++
      for (let ny = Math.max(0, y - 1); ny <= Math.min(height - 1, y + 1); ny++) for (let nx = Math.max(0, x - 1); nx <= Math.min(width - 1, x + 1); nx++) {
        const next = ny * width + nx
        if (!visited[next] && data[next * 4 + 3] > 0) { visited[next] = 1; queue.push(next) }
      }
    }
    islands.push(island)
    if (islands.length > 2048) return [] // Prefer a configured grid on noisy sheets.
  }
  const gap = Math.max(0, Math.round(options.joinGap))
  // Merge small detached effects with nearby artwork before filtering tiny islands.
  const parent = islands.map((_, index) => index)
  function root(index: number): number { while (parent[index] !== index) { parent[index] = parent[parent[index]]; index = parent[index] } return index }
  for (let i = 0; i < islands.length; i++) for (let j = i + 1; j < islands.length; j++) {
    const a = islands[i], b = islands[j]
    const dx = Math.max(0, a.x - b.right, b.x - a.right)
    const dy = Math.max(0, a.y - b.bottom, b.y - a.bottom)
    if (dx <= gap && dy <= gap) parent[root(j)] = root(i)
  }
  const groups = new Map<number, Island>()
  islands.forEach((island, index) => {
    const key = root(index), existing = groups.get(key)
    groups.set(key, existing ? { x: Math.min(existing.x, island.x), y: Math.min(existing.y, island.y), right: Math.max(existing.right, island.right), bottom: Math.max(existing.bottom, island.bottom), pixels: existing.pixels + island.pixels } : { ...island })
  })
  return [...groups.values()].filter(island => island.pixels >= Math.max(1, options.minPixels)).sort((a, b) => a.y - b.y || a.x - b.x).slice(0, 128).map((island, index) => ({ id: `suggestion-alpha-${index}`, name: `Island ${index + 1}`, x: island.x, y: island.y, width: island.right - island.x, height: island.bottom - island.y }))
}

export function acceptSuggestion(regions: Region[], suggestions: Region[], id: string, newId: string, size?: Size, maxFrames = MAX_FRAME_COUNT): { regions: Region[]; suggestions: Region[] } {
  const suggestion = suggestions.find(region => region.id === id)
  if (!suggestion || (size && !validSuggestion(suggestion, size)) || regions.length >= maxFrames || regions.some(region => sameRegion(region, suggestion))) return { regions, suggestions }
  return { regions: [...regions, { ...suggestion, id: newId }], suggestions: suggestions.filter(region => region.id !== id) }
}

export function rejectSuggestion(suggestions: Region[], id: string): Region[] { return suggestions.filter(region => region.id !== id) }

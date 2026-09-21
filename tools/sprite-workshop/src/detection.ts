import type { Pixels } from './alignment'
import type { Region, Size } from './geometry'

export type GridOptions = { rows: number; columns: number; gapX: number; gapY: number; left: number; right: number; top: number; bottom: number }
export type AlphaOptions = { joinGap: number; minPixels: number }
export type RowBoundaryMode = 'equal' | 'content'
export type RowOptions = { selection: Region; frameCount: number; boundaryMode: RowBoundaryMode; padding: number }
export type RowSuggestion = Region & { needsReview?: boolean; reviewReason?: string }
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

function equalRowRegions(selection: Region, frameCount: number, padding: number): RowSuggestion[] {
  const count = Math.max(1, Math.min(64, Math.round(frameCount)))
  if (selection.width < count) return []
  const pad = Math.max(0, Math.round(padding))
  return Array.from({ length: count }, (_, index) => {
    // Every edge is derived from the same origin, so rounding never accumulates or leaves gaps.
    const left = selection.x + Math.round(index * selection.width / count)
    const right = selection.x + Math.round((index + 1) * selection.width / count)
    const x = Math.max(selection.x, left - pad)
    const end = Math.min(selection.x + selection.width, right + pad)
    return { id: `suggestion-row-${index}`, name: `Animation ${String(index + 1).padStart(2, '0')}`, x, y: selection.y, width: end - x, height: selection.height }
  })
}

/** Suggest ordered crops inside one user-selected row. Pixel values are treated as geometry only. */
export function animationRowSuggestions(source: Pixels, options: RowOptions): RowSuggestion[] {
  const selection = options.selection
  if (!validSuggestion(selection, source)) return []
  const equal = equalRowRegions(selection, options.frameCount, options.padding)
  if (equal.length === 0) return equal

  const { data, width } = source
  const density = new Float64Array(selection.width)
  let minDensity = Infinity, maxDensity = 0
  for (let localX = 0; localX < selection.width; localX++) {
    let occupied = 0
    for (let y = selection.y; y < selection.y + selection.height; y++) {
      if (data[(y * width + selection.x + localX) * 4 + 3] > 8) occupied++
    }
    density[localX] = occupied
    minDensity = Math.min(minDensity, occupied); maxDensity = Math.max(maxDensity, occupied)
  }
  if (options.boundaryMode === 'equal') return equal.map((item, index) => {
    const left = Math.round(index * selection.width / equal.length), right = Math.round((index + 1) * selection.width / equal.length)
    const edgeInk = index > 0 && density[left] > 0 || index < equal.length - 1 && density[Math.min(selection.width - 1, right)] > 0
    return edgeInk ? { ...item, needsReview: true, reviewReason: 'Visible pixels touch an equal boundary and may be clipped.' } : item
  })
  // Fully opaque/checkerboard rows and flat alpha masks contain no trustworthy alpha valleys.
  if (maxDensity - minDensity < Math.max(2, selection.height * 0.04)) {
    return equal.map(item => ({ ...item, needsReview: true, reviewReason: 'No reliable alpha valleys; equal spacing was used.' }))
  }

  const count = equal.length, spacing = selection.width / count
  const boundaries = [0]
  let ambiguous = false
  for (let index = 1; index < count; index++) {
    const expected = index * spacing
    const radius = Math.max(2, Math.floor(spacing * 0.35))
    const start = Math.max(boundaries[index - 1] + 1, Math.floor(expected - radius))
    const end = Math.min(selection.width - (count - index), Math.ceil(expected + radius))
    let best = Math.round(expected), bestScore = Infinity
    for (let x = start; x <= end; x++) {
      const smoothed = (density[Math.max(0, x - 1)] + density[x] + density[Math.min(selection.width - 1, x + 1)]) / 3
      const score = smoothed + Math.abs(x - expected) * 0.08
      if (score < bestScore) { bestScore = score; best = x }
    }
    if (!Number.isFinite(bestScore) || bestScore > selection.height * 0.3) ambiguous = true
    boundaries.push(best)
  }
  boundaries.push(selection.width)
  if (ambiguous) return equal.map(item => ({ ...item, needsReview: true, reviewReason: 'Boundaries were ambiguous; equal spacing was used.' }))

  const pad = Math.max(0, Math.round(options.padding))
  return Array.from({ length: count }, (_, index) => {
    const rawLeft = boundaries[index], rawRight = boundaries[index + 1]
    const left = Math.max(0, rawLeft - pad), right = Math.min(selection.width, rawRight + pad)
    const edgeInk = index > 0 && density[rawLeft] > 0 || index < count - 1 && density[Math.min(selection.width - 1, rawRight)] > 0
    return {
      id: `suggestion-row-${index}`, name: `Animation ${String(index + 1).padStart(2, '0')}`,
      x: selection.x + left, y: selection.y, width: right - left, height: selection.height,
      ...(edgeInk ? { needsReview: true, reviewReason: 'Visible pixels touch a proposed boundary and may be clipped.' } : {}),
    }
  })
}

export function acceptSuggestionBatch(regions: Region[], suggestions: Region[], size: Size, createId: () => string, maxFrames = MAX_FRAME_COUNT): { regions: Region[]; added: Region[]; error: string | null } {
  if (regions.length + suggestions.length > maxFrames) return { regions, added: [], error: `Adding this batch would exceed the ${maxFrames}-frame limit.` }
  const pending: Region[] = []
  for (const suggestion of suggestions) {
    if (!validSuggestion(suggestion, size)) return { regions, added: [], error: `${suggestion.name} has invalid source coordinates.` }
    if ([...regions, ...pending].some(region => sameRegion(region, suggestion))) return { regions, added: [], error: `${suggestion.name} duplicates an existing or proposed frame.` }
    pending.push({ ...suggestion, id: createId() })
  }
  return { regions: [...regions, ...pending], added: pending, error: null }
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

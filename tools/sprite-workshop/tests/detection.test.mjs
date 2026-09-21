import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/detection.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { gridSuggestions, alphaSuggestions, animationRowSuggestions, acceptSuggestion, acceptAllSuggestions, acceptSuggestionBatch, eligibleSuggestionCount, rejectSuggestion } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

function pixels(width, height, opaque = []) {
  const data = new Uint8ClampedArray(width * height * 4)
  for (const [left, top, right, bottom] of opaque) for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) data[(y * width + x) * 4 + 3] = 255
  return { data, width, height }
}

test('animation row divides eight ordered frames without gaps or accumulated rounding', () => {
  const source = pixels(103, 20)
  const suggestions = animationRowSuggestions(source, { selection: { id: 'row', name: 'row', x: 3, y: 4, width: 97, height: 11 }, frameCount: 8, boundaryMode: 'equal', padding: 0 })
  assert.equal(suggestions.length, 8)
  assert.deepEqual(suggestions.map(item => item.name), ['Animation 01', 'Animation 02', 'Animation 03', 'Animation 04', 'Animation 05', 'Animation 06', 'Animation 07', 'Animation 08'])
  assert.equal(suggestions[0].x, 3)
  assert.equal(suggestions.at(-1).x + suggestions.at(-1).width, 100)
  assert.ok(suggestions.every((item, index) => item.y === 4 && item.height === 11 && (!index || suggestions[index - 1].x + suggestions[index - 1].width === item.x)))
  assert.deepEqual(suggestions.map(item => item.width), [12, 12, 12, 13, 12, 12, 12, 12])
  const padded = animationRowSuggestions(source, { selection: { id: 'row', name: 'row', x: 3, y: 4, width: 97, height: 11 }, frameCount: 8, boundaryMode: 'equal', padding: 2 })
  assert.equal(padded[0].x, 3)
  assert.ok(padded[0].x + padded[0].width > padded[1].x, 'padding expands crops without dropping boundary pixels')
  assert.equal(padded.at(-1).x + padded.at(-1).width, 100)
})

test('content-assisted row supports irregular widths and retains detached effects in the ordered frame batch', () => {
  const source = pixels(80, 16, [[2, 4, 13, 14], [16, 1, 18, 3], [23, 3, 38, 14], [44, 4, 52, 14], [57, 2, 76, 14]])
  const suggestions = animationRowSuggestions(source, { selection: { id: 'row', name: 'row', x: 0, y: 0, width: 80, height: 16 }, frameCount: 4, boundaryMode: 'content', padding: 0 })
  assert.equal(suggestions.length, 4)
  assert.ok(suggestions[0].x <= 16 && suggestions[0].x + suggestions[0].width > 16, 'detached effect remains with the first constrained frame')
  assert.ok(new Set(suggestions.map(item => item.width)).size > 1, 'visible-width valleys can produce irregular crops')
  assert.ok(suggestions.every((item, index) => !index || suggestions[index - 1].x + suggestions[index - 1].width === item.x))
})

test('opaque presentation backgrounds use an explicit equal-spacing fallback', () => {
  const source = pixels(64, 12, [[0, 0, 64, 12]])
  const suggestions = animationRowSuggestions(source, { selection: { id: 'row', name: 'row', x: 0, y: 0, width: 64, height: 12 }, frameCount: 4, boundaryMode: 'content', padding: 0 })
  assert.deepEqual(suggestions.map(item => item.width), [16, 16, 16, 16])
  assert.ok(suggestions.every(item => item.needsReview && /equal spacing/.test(item.reviewReason)))
})

test('equal boundaries flag visible artwork that may be clipped', () => {
  const source = pixels(32, 10, [[7, 1, 10, 9]])
  const suggestions = animationRowSuggestions(source, { selection: { id: 'row', name: 'row', x: 0, y: 0, width: 32, height: 10 }, frameCount: 4, boundaryMode: 'equal', padding: 0 })
  assert.equal(suggestions[0].needsReview, true)
  assert.match(suggestions[0].reviewReason, /clipped/)
})

test('grid suggestions respect rows, columns, spacing and asymmetric margins', () => {
  const suggestions = gridSuggestions({ width: 50, height: 30 }, { rows: 2, columns: 3, gapX: 2, gapY: 4, left: 3, right: 5, top: 2, bottom: 4 })
  assert.equal(suggestions.length, 6)
  assert.deepEqual([suggestions[0].x, suggestions[0].y, suggestions[0].width, suggestions[0].height], [3, 2, 13, 10])
  assert.equal(suggestions.at(-1).x + suggestions.at(-1).width, 45)
  assert.equal(suggestions.at(-1).y + suggestions.at(-1).height, 26)
  assert.deepEqual(gridSuggestions({ width: 10, height: 10 }, { rows: 2, columns: 2, gapX: 8, gapY: 8, left: 2, right: 2, top: 2, bottom: 2 }), [])
})

test('alpha islands join nearby detached effects and remain conservative for distant art', () => {
  const data = new Uint8ClampedArray(20 * 8 * 4)
  for (const [x, y] of [[2, 2], [3, 2], [5, 1], [16, 5], [17, 5]]) data[(y * 20 + x) * 4 + 3] = 1
  const suggestions = alphaSuggestions({ data, width: 20, height: 8 }, { joinGap: 2, minPixels: 2 })
  assert.equal(suggestions.length, 2)
  assert.deepEqual([suggestions[0].x, suggestions[0].y, suggestions[0].width, suggestions[0].height], [2, 1, 4, 2])
})

test('suggestions are preview-only until accepted; rejection preserves manual frames', () => {
  const manual = [{ id: 'manual', name: 'Handmade', x: 1, y: 1, width: 2, height: 2 }]
  const suggestions = gridSuggestions({ width: 8, height: 4 }, { rows: 1, columns: 2, gapX: 0, gapY: 0, left: 0, right: 0, top: 0, bottom: 0 })
  assert.equal(manual.length, 1)
  const accepted = acceptSuggestion(manual, suggestions, suggestions[0].id, 'new')
  assert.deepEqual(accepted.regions[0], manual[0])
  assert.equal(accepted.regions[1].id, 'new')
  assert.equal(accepted.suggestions.length, 1)
  assert.deepEqual(rejectSuggestion(accepted.suggestions, accepted.suggestions[0].id), [])
  assert.equal(manual.length, 1)
})

test('bulk acceptance adds only valid unique source regions and reports every outcome', () => {
  const manual = [{ id: 'manual', name: 'Handmade', x: 0, y: 0, width: 4, height: 4 }]
  const suggestions = [
    { id: 'duplicate', name: 'Duplicate', x: 0, y: 0, width: 4, height: 4 },
    { id: 'valid', name: 'Valid', x: 4, y: 0, width: 4, height: 4 },
    { id: 'repeated', name: 'Repeated', x: 4, y: 0, width: 4, height: 4 },
    { id: 'outside', name: 'Outside', x: 7, y: 0, width: 2, height: 4 },
  ]
  let id = 0
  assert.equal(eligibleSuggestionCount(manual, suggestions, { width: 8, height: 4 }), 1)
  const result = acceptAllSuggestions(manual, suggestions, { width: 8, height: 4 }, () => `new-${++id}`)
  assert.deepEqual(result.regions, [manual[0], { ...suggestions[1], id: 'new-1' }])
  assert.deepEqual([result.added, result.skipped, result.rejected], [1, 2, 1])
  assert.deepEqual(result.suggestions, [])
  assert.deepEqual(manual, [{ id: 'manual', name: 'Handmade', x: 0, y: 0, width: 4, height: 4 }])
})

test('bulk acceptance respects the frame limit and repeated acceptance adds nothing', () => {
  const existing = [{ id: 'a', name: 'A', x: 0, y: 0, width: 1, height: 1 }]
  const suggestions = [{ id: 'b', name: 'B', x: 1, y: 0, width: 1, height: 1 }, { id: 'c', name: 'C', x: 2, y: 0, width: 1, height: 1 }]
  const first = acceptAllSuggestions(existing, suggestions, { width: 3, height: 1 }, () => 'new', 2)
  assert.deepEqual([first.added, first.skipped, first.rejected], [1, 0, 1])
  const repeated = acceptAllSuggestions(first.regions, suggestions, { width: 3, height: 1 }, () => 'unused', 2)
  assert.deepEqual([repeated.added, repeated.skipped, repeated.rejected], [0, 1, 1])
  assert.equal(repeated.regions.length, 2)
})

test('ordered batch acceptance is atomic and creates stable IDs only after validation', () => {
  const existing = [{ id: 'old', name: 'Old', x: 0, y: 0, width: 2, height: 2 }]
  const proposed = [{ id: 'p1', name: 'Animation 01', x: 2, y: 0, width: 2, height: 2 }, { id: 'p2', name: 'Animation 02', x: 4, y: 0, width: 2, height: 2 }]
  let id = 0
  const accepted = acceptSuggestionBatch(existing, proposed, { width: 6, height: 2 }, () => `stable-${++id}`)
  assert.deepEqual(accepted.added.map(item => item.id), ['stable-1', 'stable-2'])
  assert.deepEqual(accepted.regions.map(item => item.name), ['Old', 'Animation 01', 'Animation 02'])
  const invalid = acceptSuggestionBatch(existing, [...proposed, { ...proposed[1], id: 'bad', x: 5 }], { width: 6, height: 2 }, () => 'unused')
  assert.equal(invalid.error !== null, true)
  assert.strictEqual(invalid.regions, existing)
  assert.deepEqual(invalid.added, [])
})

import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/detection.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { gridSuggestions, alphaSuggestions, acceptSuggestion, acceptAllSuggestions, eligibleSuggestionCount, rejectSuggestion } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

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

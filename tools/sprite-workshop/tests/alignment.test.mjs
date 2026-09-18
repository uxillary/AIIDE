import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/alignment.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { calculateLayout, emptyOffsets } = await import(`data:text/javascript,${encodeURIComponent(code)}`)
const region = (id, x, y, width, height) => ({ id, name: id, x, y, width, height })
const options = { mode: 'bottom', padding: 3, minWidth: 0, minHeight: 0 }

test('all eight slots share one fixed canvas and anchor based on exact crop dimensions', () => {
  const frames = [region('short', 10, 2, 8, 6), region('tall', 30, 5, 12, 14), ...Array(6).fill(null)]
  const layout = calculateLayout(frames, emptyOffsets(), options)
  assert.deepEqual([layout.width, layout.height, layout.anchorX, layout.anchorY], [18, 20, 9, 17])
  assert.equal(layout.placements[0].y + 6, layout.placements[1].y + 14)
  assert.deepEqual(layout.placements[0].source, { x: 10, y: 2, width: 8, height: 6 })
  assert.equal(layout.placements.length, 8)
  assert.equal(layout.placements[7], null)
})

test('offsets move only the chosen slot without changing canvas origin or source crop', () => {
  const frames = [region('a', 10, 4, 8, 8), region('b', 30, 4, 8, 8)]
  const offsets = emptyOffsets()
  const before = calculateLayout(frames, offsets, options)
  offsets[1] = { x: 7, y: -4 }
  const after = calculateLayout(frames, offsets, options)
  assert.deepEqual([after.width, after.height, after.anchorX, after.anchorY], [before.width, before.height, before.anchorX, before.anchorY])
  assert.deepEqual(after.placements[0], before.placements[0])
  assert.deepEqual(after.placements[1].source, before.placements[1].source)
  assert.deepEqual([after.placements[1].x - before.placements[1].x, after.placements[1].y - before.placements[1].y], [7, -4])
  assert.deepEqual(frames[1], region('b', 30, 4, 8, 8))
})

test('identical crops in different slots can carry intentional motion', () => {
  const frame = region('jump', 10, 20, 16, 16)
  const offsets = emptyOffsets()
  offsets[1] = { x: 0, y: -5 }
  const layout = calculateLayout([frame, frame], offsets, { ...options, minWidth: 40, minHeight: 48 })
  assert.deepEqual([layout.width, layout.height], [40, 48])
  assert.equal(layout.placements[1].y - layout.placements[0].y, -5)
})

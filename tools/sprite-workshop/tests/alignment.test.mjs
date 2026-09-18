import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/alignment.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { alignmentDragOffset, alignmentScale, calculateLayout, emptyOffsets, referenceSlotFor } = await import(`data:text/javascript,${encodeURIComponent(code)}`)
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

test('zoom and pan are view-only; exact drag and nudges use source pixels', () => {
  const frames = [region('a', 10, 4, 8, 8)]
  const offsets = emptyOffsets()
  const before = calculateLayout(frames, offsets, options)
  for (const zoom of [1, 2, 4, 8]) {
    assert.equal(alignmentScale(zoom, before, { width: 300, height: 200 }), zoom)
    assert.deepEqual(alignmentDragOffset({ x: 0, y: 0 }, zoom, -zoom, zoom), { x: 1, y: -1 })
    assert.deepEqual(alignmentDragOffset({ x: 0, y: 0 }, zoom * 0.49, 0, zoom), { x: 0, y: 0 })
  }
  assert.equal(alignmentScale('fit', { width: 40, height: 30 }, { width: 150, height: 120 }), 3)
  assert.deepEqual(calculateLayout(frames, offsets, options), before)
  offsets[0] = { x: 1, y: -1 } // arrow and button actions add one regardless of view scale
  const after = calculateLayout(frames, offsets, options)
  assert.deepEqual([after.placements[0].x - before.placements[0].x, after.placements[0].y - before.placements[0].y], [1, -1])
  assert.deepEqual(after.placements[0].source, before.placements[0].source)
})

test('previous and fixed reference slots use their shared canvas placements', () => {
  const offsets = emptyOffsets()
  offsets[2] = { x: 5, y: -3 }
  const layout = calculateLayout([region('a', 0, 0, 8, 8), null, region('b', 20, 20, 8, 8)], offsets, options)
  assert.equal(referenceSlotFor(2, 'previous', 0), 1)
  assert.equal(referenceSlotFor(2, 'fixed', 0), 0)
  assert.equal(referenceSlotFor(0, 'previous', 2), 7)
  assert.deepEqual(layout.placements[referenceSlotFor(2, 'fixed', 0)], layout.placements[0])
  assert.deepEqual([layout.placements[2].x - layout.placements[0].x, layout.placements[2].y - layout.placements[0].y], [5, -3])
})

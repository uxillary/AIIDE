import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const compile = async path => ts.transpileModule(await readFile(new URL(path, import.meta.url), 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const touchups = await import(`data:text/javascript,${encodeURIComponent(await compile('../src/touchups.ts'))}`)
const render = await import(`data:text/javascript,${encodeURIComponent(await compile('../src/render.ts'))}`)
const { appendErase, eraseRectFromPoints, framePoint, resetFrameTouchUps, stepTouchUpHistory, validateTouchUps } = touchups
const { drawFrame, drawPlacement } = render

test('rectangle coordinates are frame-local, bounded, and independent of display zoom', () => {
  const size = { width: 16, height: 12 }
  assert.deepEqual(framePoint({ x: 140, y: 90 }, { left: 100, top: 50, width: 80, height: 60 }, size), { x: 8, y: 8 })
  assert.deepEqual(framePoint({ x: 180, y: 130 }, { left: 100, top: 50, width: 160, height: 120 }, size), { x: 8, y: 8 })
  assert.deepEqual(eraseRectFromPoints({ x: 14, y: 10 }, { x: 30, y: -4 }, size), { x: 14, y: 0, width: 2, height: 11 })
})

test('touch-ups are keyed by stable frame ID and reset without changing frame data', () => {
  const frame = { id: 'frame-a', name: 'A', x: 20, y: 30, width: 8, height: 6 }
  const original = structuredClone(frame)
  const edits = appendErase({}, frame.id, { x: 1, y: 2, width: 3, height: 2 })
  assert.deepEqual(validateTouchUps(edits, [frame]), edits)
  assert.deepEqual(frame, original)
  assert.deepEqual(resetFrameTouchUps(edits, frame.id), {})
  assert.throws(() => validateTouchUps({ [frame.id]: [{ x: 7, y: 5, width: 2, height: 2 }] }, [frame]), /outside their frame bounds/)
})

test('touch-up history supports undo, redo, and undoable reset', () => {
  const empty = {}
  const erased = { frame: [{ x: 1, y: 1, width: 2, height: 2 }] }
  const reset = {}
  const undoErase = stepTouchUpHistory([empty], [], erased)
  assert.deepEqual(undoErase.touchUps, empty)
  const redoErase = stepTouchUpHistory(undoErase.to, undoErase.from, undoErase.touchUps)
  assert.deepEqual(redoErase.touchUps, erased)
  const undoReset = stepTouchUpHistory([empty, erased], [], reset)
  assert.deepEqual(undoReset.touchUps, erased)
})

test('shared compositor clears erased pixels to transparency and placements use the same edited frame', () => {
  const calls = []
  const source = { immutable: true }
  const compositeContext = {
    set imageSmoothingEnabled(value) { calls.push(['composite-smoothing', value]) },
    drawImage: (...args) => calls.push(['composite-draw', ...args]),
    clearRect: (...args) => calls.push(['clear', ...args]),
  }
  const composite = { width: 0, height: 0, getContext: () => compositeContext }
  globalThis.document = { createElement: name => { assert.equal(name, 'canvas'); return composite } }
  const destination = { set imageSmoothingEnabled(value) { calls.push(['destination-smoothing', value]) }, drawImage: (...args) => calls.push(['destination-draw', ...args]) }
  const frame = { id: 'frame-a', name: 'A', x: 10, y: 12, width: 8, height: 6 }
  const edits = { [frame.id]: [{ x: 2, y: 1, width: 3, height: 2 }] }
  drawFrame(destination, source, frame, edits[frame.id], 0, 0)
  assert.deepEqual(calls.find(call => call[0] === 'clear'), ['clear', 2, 1, 3, 2])
  assert.equal(source.immutable, true)
  calls.length = 0
  drawPlacement(destination, source, { source: frame, frameId: frame.id, x: 4, y: 5 }, edits)
  assert.deepEqual(calls.find(call => call[0] === 'clear'), ['clear', 2, 1, 3, 2])
  assert.deepEqual(calls.at(-1).slice(-4), [4, 5, 8, 6])
})

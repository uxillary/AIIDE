import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/frameActions.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { removeFrame, restoreFrame } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

test('deleting a frame clears all references, selects a neighbour and undo restores them', () => {
  const regions = ['a', 'b', 'c'].map(id => ({ id, name: id, x: 0, y: 0, width: 1, height: 1 }))
  const before = { regions, slots: ['a', 'b', 'c', 'b', null, 'b', null, null], selectedId: 'b' }
  const removed = removeFrame(before, 'b')
  assert.deepEqual(removed.state.regions.map(item => item.id), ['a', 'c'])
  assert.deepEqual(removed.state.slots, ['a', null, 'c', null, null, null, null, null])
  assert.equal(removed.state.selectedId, 'c')
  const restored = restoreFrame(removed.state, removed.undo)
  assert.deepEqual(restored.regions, regions)
  assert.deepEqual(restored.slots, before.slots)
  assert.equal(restored.selectedId, 'b')
  const withNewAssignment = { ...removed.state, slots: ['a', 'c', 'c', null, null, null, null, null] }
  assert.equal(restoreFrame(withNewAssignment, removed.undo).slots[1], 'c')
})

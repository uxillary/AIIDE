import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

let source = await readFile(new URL('../src/project.ts', import.meta.url), 'utf8')
source = source.replace("import { loadPng } from './image'", 'const loadPng = undefined')
const touchupsSource = await readFile(new URL('../src/touchups.ts', import.meta.url), 'utf8')
const touchupsCode = ts.transpileModule(touchupsSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const touchupsUrl = `data:text/javascript,${encodeURIComponent(touchupsCode)}`
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
  .replace("'./touchups'", JSON.stringify(touchupsUrl))
const projectModule = await import(`data:text/javascript,${encodeURIComponent(code)}`)
const { PROJECT_SCHEMA_VERSION, deserializePortableProject, loadLatestProject, saveLatestProject, saveWithStatus, serializePortableProject, validateProject } = projectModule

const regions = [
  { id: 'frame-b', name: 'Second', x: 5, y: 6, width: 7, height: 8 },
  { id: 'frame-a', name: 'First', x: 1, y: 2, width: 3, height: 4 },
]
const project = {
  schemaVersion: PROJECT_SCHEMA_VERSION, regions, selectedId: 'frame-a', slots: ['frame-a', 'frame-b', null, null, null, null, null, null], activeSlot: 1,
  fps: 12, animationName: 'walk', alignmentMode: 'center', offsets: [{ x: 4, y: -2 }, ...Array.from({ length: 7 }, () => ({ x: 0, y: 0 }))],
  padding: 9, minWidth: 32, minHeight: 48, onion: true, onionReference: 'fixed', fixedReferenceSlot: 1, centreGuide: false, baselineGuide: true,
  pixelGrid: true, baselineOffset: 3, detectionMode: 'row', grid: { rows: 2, columns: 3, gapX: 1, gapY: 2, left: 3, right: 4, top: 5, bottom: 6 }, joinGap: 7, minPixels: 8,
  rowSelection: { id: 'row-selection', name: 'Animation row', x: 10, y: 20, width: 80, height: 16 }, rowFrameCount: 8, rowBoundaryMode: 'content', rowPadding: 2,
  touchUps: { 'frame-a': [{ x: 1, y: 1, width: 2, height: 2 }] },
}
const png = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3])
const record = { id: 'latest', schemaVersion: PROJECT_SCHEMA_VERSION, savedAt: 1, project, source: { blob: new Blob([png], { type: 'image/png' }), name: 'elma.png', type: 'image/png', size: png.length, lastModified: 42 } }

test('portable project round trip preserves source bytes, frame order and stable IDs, slots and alignment', async () => {
  const restored = deserializePortableProject(await serializePortableProject(record))
  assert.deepEqual(new Uint8Array(await restored.source.blob.arrayBuffer()), png)
  assert.deepEqual(restored.project.regions, regions)
  assert.deepEqual(restored.project.slots, project.slots)
  assert.equal(restored.project.alignmentMode, 'center')
  assert.deepEqual(restored.project.offsets[0], { x: 4, y: -2 })
  assert.equal(restored.project.padding, 9)
  assert.deepEqual(restored.project.rowSelection, project.rowSelection)
  assert.equal(restored.project.rowBoundaryMode, 'content')
  assert.deepEqual(restored.project.touchUps, project.touchUps)
})

test('schema version 1 projects without newer optional settings load with empty touch-ups', () => {
  const { rowSelection, rowFrameCount, rowBoundaryMode, rowPadding, touchUps, ...legacy } = project
  assert.deepEqual(validateProject(legacy), { ...legacy, touchUps: {} })
})

test('malformed and unsupported portable projects are rejected', () => {
  assert.throws(() => deserializePortableProject('{bad'), /valid Sprite Workshop/)
  assert.throws(() => deserializePortableProject(JSON.stringify({ kind: 'aiide-sprite-workshop-project', schemaVersion: 99 })), /Unsupported project schema/)
  assert.throws(() => validateProject({ ...project, slots: ['missing'] }), /slot assignments/)
  assert.throws(() => validateProject({ ...project, touchUps: { 'frame-a': [{ x: 2, y: 0, width: 2, height: 1 }] } }), /outside their frame bounds/)
})

test('save status becomes saved only after success and reports failure', async () => {
  const success = []
  let resolveSave
  const pending = saveWithStatus(() => new Promise(resolve => { resolveSave = resolve }), value => success.push(value))
  assert.deepEqual(success, ['saving'])
  resolveSave(); await pending
  assert.deepEqual(success, ['saving', 'saved'])
  const failure = []
  await assert.rejects(saveWithStatus(() => Promise.reject(new Error('quota')), value => failure.push(value)), /quota/)
  assert.deepEqual(failure, ['saving', 'failed'])
})

test('IndexedDB latest project save and reload recovery retain the complete record', async () => {
  const records = new Map()
  globalThis.indexedDB = {
    open() {
      const request = { result: null, error: null, onsuccess: null, onerror: null, onupgradeneeded: null }
      queueMicrotask(() => {
        const database = {
          objectStoreNames: { contains: () => true }, close() {},
          transaction(_name, mode) {
            const transaction = { error: null, oncomplete: null, onabort: null, onerror: null, objectStore: () => ({
              put(value) { records.set(value.id, value); setTimeout(() => transaction.oncomplete?.(), 0) },
              get(key) { const item = { result: undefined, error: null, onsuccess: null, onerror: null }; queueMicrotask(() => { item.result = records.get(key); item.onsuccess?.() }); setTimeout(() => transaction.oncomplete?.(), 0); return item },
            }) }
            return transaction
          },
        }
        request.result = database; request.onsuccess?.()
      })
      return request
    },
  }
  await saveLatestProject(record)
  const restored = await loadLatestProject()
  assert.deepEqual(restored.project, project)
  assert.deepEqual(new Uint8Array(await restored.source.blob.arrayBuffer()), png)
})

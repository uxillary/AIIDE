import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/export.ts', import.meta.url), 'utf8')
const renderSource = await readFile(new URL('../src/render.ts', import.meta.url), 'utf8')
const renderCode = ts.transpileModule(renderSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const renderUrl = `data:text/javascript,${encodeURIComponent(renderCode)}`
const { drawPlacement } = await import(renderUrl)
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText.replace("'./render'", JSON.stringify(renderUrl))
const { cropToCanvas, alignedCanvas, pngBlob } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

test('PNG crop draws only the selected source pixels on a transparent canvas', async () => {
  const calls = []
  const canvas = { width: 0, height: 0, getContext: () => ({ set imageSmoothingEnabled(value) { calls.push(['smoothing', value]) }, drawImage: (...args) => calls.push(['drawImage', ...args]) }), toBlob: (callback, format) => { calls.push(['toBlob', format]); callback(new Blob(['png'], { type: format })) } }
  globalThis.document = { createElement: name => { assert.equal(name, 'canvas'); return canvas } }
  const image = { width: 128, height: 128 }
  const region = { id: 'id', name: 'frame', x: 12, y: 18, width: 32, height: 24 }
  const result = cropToCanvas(image, region)
  assert.deepEqual([result.width, result.height], [32, 24])
  assert.deepEqual(calls, [['smoothing', false], ['drawImage', image, 12, 18, 32, 24, 0, 0, 32, 24]])
  const blob = await pngBlob(result)
  assert.equal(blob.type, 'image/png')
  assert.deepEqual(calls.at(-1), ['toBlob', 'image/png'])
})

test('aligned PNG uses the preview placement on a transparent shared canvas', () => {
  const calls = []
  const canvas = { width: 0, height: 0, getContext: () => ({ set imageSmoothingEnabled(value) { calls.push(['smoothing', value]) }, drawImage: (...args) => calls.push(['drawImage', ...args]) }) }
  globalThis.document = { createElement: () => canvas }
  const image = { width: 100, height: 100 }
  const layout = { width: 50, height: 60, placements: [{ source: { x: 14, y: 16, width: 20, height: 22 }, x: 13, y: 18 }] }
  const result = alignedCanvas(image, layout, 0)
  assert.deepEqual([result.width, result.height], [50, 60])
  assert.deepEqual(calls, [['smoothing', false], ['drawImage', image, 14, 16, 20, 22, 13, 18, 20, 22]])
})

test('stepping and playback render the same integer placement as aligned export', () => {
  const placement = { source: { x: 5, y: 7, width: 12, height: 14 }, x: 19, y: 11 }
  const layout = { width: 48, height: 48, placements: [null, placement] }
  const image = { width: 100, height: 100 }
  const draws = []
  const context = { set imageSmoothingEnabled(value) { draws.push(['smoothing', value]) }, drawImage: (...args) => draws.push(['drawImage', ...args]) }
  drawPlacement(context, image, layout.placements[1]) // same renderer used when stepping and playing
  const previewCalls = [...draws]
  draws.length = 0
  globalThis.document = { createElement: () => ({ width: 0, height: 0, getContext: () => context }) }
  alignedCanvas(image, layout, 1)
  assert.deepEqual(draws, previewCalls)
  assert.deepEqual(draws.at(-1), ['drawImage', image, 5, 7, 12, 14, 19, 11, 12, 14])
})

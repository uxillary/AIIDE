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
const { cropToCanvas, alignedCanvas, pngBlob, spriteSheetCanvas, animationMetadata, validateAnimation, sanitizeFilename, downloadAnimation } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

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

test('aligned PNG uses the shared canvas placement without preview guides', () => {
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

const frames = Array.from({ length: 8 }, (_, i) => ({ id: `frame-${i}`, name: `Frame ${i}`, x: i * 10, y: 3, width: 4, height: 5 }))
const slots = frames.map(frame => frame.id)
const layout = {
  width: 20, height: 16,
  placements: frames.map((frame, i) => ({ source: { x: frame.x, y: frame.y, width: 4, height: 5 }, frameId: frame.id, x: 2 + i, y: i })),
}

test('crop, aligned slot, and sprite sheet exports all apply the same frame erases', () => {
  const clears = []
  globalThis.document = {
    createElement: () => ({
      width: 0, height: 0,
      getContext: () => ({
        set imageSmoothingEnabled(_value) {}, drawImage() {},
        clearRect: (...args) => clears.push(args),
      }),
    }),
  }
  const image = { width: 100, height: 100 }
  const touchUps = { 'frame-0': [{ x: 1, y: 2, width: 2, height: 2 }] }
  cropToCanvas(image, frames[0], touchUps)
  alignedCanvas(image, layout, 0, touchUps)
  spriteSheetCanvas(image, layout, touchUps)
  assert.deepEqual(clears, Array.from({ length: 3 }, () => [1, 2, 2, 2]))
})

test('sheet uses eight ordered fixed cells, exact offsets, and transparent backgrounds', () => {
  const draws = []
  const canvas = { width: 0, height: 0, getContext: () => ({ set imageSmoothingEnabled(value) { draws.push(['smoothing', value]) }, drawImage: (...args) => draws.push(['drawImage', ...args]) }) }
  globalThis.document = { createElement: name => { assert.equal(name, 'canvas'); return canvas } }
  const image = { width: 100, height: 100 }
  const sheet = spriteSheetCanvas(image, layout)
  assert.deepEqual([sheet.width, sheet.height], [160, 16])
  assert.equal(draws.length, 16)
  assert.equal(draws.filter(call => call[0] === 'drawImage').length, 8)
  for (let i = 0; i < 8; i++) {
    assert.deepEqual(draws[i * 2], ['smoothing', false])
    assert.deepEqual(draws[i * 2 + 1], ['drawImage', image, i * 10, 3, 4, 5, i * 20 + 2 + i, i, 4, 5])
  }
  // No fill, checkerboard, clear with a color, or independently resized frame.
})

test('metadata rectangles, timing, and filenames match the PNG cell dimensions', () => {
  const meta = animationMetadata('Elma walk', 8, layout)
  assert.deepEqual([meta.schemaVersion, meta.frameCount, meta.fps, meta.frameDurationMs, meta.layout], [1, 8, 8, 125, 'horizontal'])
  assert.equal(meta.spriteSheet, 'Elma-walk.png')
  assert.deepEqual(meta.sheet, { width: 160, height: 16 })
  assert.deepEqual(meta.cell, { width: 20, height: 16 })
  assert.deepEqual(meta.frames, Array.from({ length: 8 }, (_, i) => ({ slot: i + 1, rect: { x: i * 20, y: 0, width: 20, height: 16 } })))
  assert.equal(meta.frames.at(-1).rect.x + meta.cell.width, meta.sheet.width)
})

test('validation identifies empty, deleted, and clipped slots without modifying placements', () => {
  assert.equal(validateAnimation(slots, frames, layout, 'walk', 8), null)
  assert.match(validateAnimation(slots.map((id, i) => i === 3 ? null : id), frames, layout, 'walk', 8), /Slot 4 is empty/)
  assert.match(validateAnimation(slots, frames.filter(frame => frame.id !== slots[5]), layout, 'walk', 8), /Slot 6 references a deleted frame/)
  const clipped = structuredClone(layout)
  clipped.placements[7].x = 19
  assert.match(validateAnimation(slots, frames, clipped, 'walk', 8), /Slot 8 extends outside/)
  assert.equal(layout.placements[7].x, 9)
  assert.match(validateAnimation(slots, frames, layout, ' ', 8), /animation name/)
})

test('filenames are safe, bounded, and have a fallback', () => {
  assert.equal(sanitizeFilename(' ../Elma: run? \\ '), 'Elma-run')
  assert.equal(sanitizeFilename('💚 / '), 'animation')
  assert.ok(sanitizeFilename('x'.repeat(200)).length <= 80)
})

test('failed PNG encoding triggers no partial download; successful files share dimensions and name', async () => {
  const downloads = []
  const revoked = []
  const blobs = []
  const canvases = []
  let shouldFail = true
  globalThis.document = {
    createElement: name => name === 'canvas'
      ? (() => { const canvas = { width: 0, height: 0, getContext: () => ({ set imageSmoothingEnabled(_value) {}, drawImage() {} }), toBlob: (callback, format) => callback(shouldFail ? null : new Blob(['png'], { type: format })) }; canvases.push(canvas); return canvas })()
      : { click() { downloads.push([this.download, this.href]) }, remove() {} },
    body: { append() {} },
  }
  globalThis.URL.createObjectURL = blob => { blobs.push(blob); return `blob:${blob.type}:${blobs.length}` }
  globalThis.URL.revokeObjectURL = url => revoked.push(url)
  globalThis.window = { setTimeout: callback => { callback() } }
  await assert.rejects(downloadAnimation({}, slots, frames, layout, 'Walk!', 8), /PNG export failed/)
  assert.equal(downloads.length, 0)
  shouldFail = false
  await downloadAnimation({}, slots, frames, layout, 'Walk!', 8)
  assert.deepEqual(downloads.map(([name]) => name), ['Walk.png', 'Walk.json'])
  assert.equal(revoked.length, 2)
  const json = JSON.parse(await blobs[1].text())
  assert.deepEqual(json.sheet, { width: canvases.at(-1).width, height: canvases.at(-1).height })
  assert.equal(json.spriteSheet, downloads[0][0])
  assert.equal(blobs[0].type, 'image/png')
})

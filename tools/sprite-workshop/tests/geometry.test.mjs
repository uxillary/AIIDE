import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/geometry.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { boundedRegion, regionFromCorners, resizeRegion, screenToImage, zoomAt } = await import(`data:text/javascript,${encodeURIComponent(code)}`)

test('screen coordinates map to source pixels at fractional and enlarged zoom', () => {
  const size = { width: 100, height: 80 }
  assert.deepEqual(screenToImage({ x: 35, y: 50 }, { x: 10, y: 20 }, 0.5, size), { x: 50, y: 60 })
  assert.deepEqual(screenToImage({ x: 110, y: 140 }, { x: 10, y: 20 }, 2, size), { x: 50, y: 60 })
  assert.deepEqual(screenToImage({ x: 210, y: 260 }, { x: 10, y: 20 }, 4, size), { x: 50, y: 60 })
  assert.deepEqual(screenToImage({ x: -20, y: 1000 }, { x: 10, y: 20 }, 2, size), { x: 0, y: 79 })
})

test('zoom keeps the pixel below the pointer fixed', () => {
  const pan = { x: -40, y: 15 }, pointer = { x: 80, y: 95 }
  const before = screenToImage(pointer, pan, 2, { width: 500, height: 500 })
  const after = screenToImage(pointer, zoomAt(pan, pointer, 2, 5), 5, { width: 500, height: 500 })
  assert.deepEqual(after, before)
})

test('drag, numeric bounds and resize stay within original image', () => {
  const size = { width: 100, height: 80 }
  const region = regionFromCorners({ x: 20, y: 30 }, { x: 10, y: 15 }, 'a', 'Frame')
  assert.deepEqual([region.x, region.y, region.width, region.height], [10, 15, 11, 16])
  const bounded = boundedRegion({ ...region, x: 95, y: 78, width: 30, height: 30 }, size)
  assert.deepEqual([bounded.x, bounded.y, bounded.width, bounded.height], [95, 78, 5, 2])
  const resized = resizeRegion(region, 'nw', 100, 100, size)
  assert.deepEqual([resized.x, resized.y, resized.width, resized.height], [20, 30, 1, 1])
})

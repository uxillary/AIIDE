import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/alignment.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { visibleBounds, calculateLayout } = await import(`data:text/javascript,${encodeURIComponent(code)}`)
const region = (id, x, y, width, height, anchorX = 0, anchorY = 0) => ({ id, name: id, x, y, width, height, anchorX, anchorY })
const image = (width, height, points) => {
  const data = new Uint8ClampedArray(width * height * 4)
  for (const [x, y, alpha] of points) data[(y * width + x) * 4 + 3] = alpha
  return { width, height, data }
}

test('alpha bounds ignore only fully transparent padding, including partial effects', () => {
  const pixels = image(12, 9, [[2, 3, 255], [8, 1, 1], [5, 7, 128]])
  assert.deepEqual(visibleBounds(pixels, region('a', 1, 0, 9, 9)), { x: 2, y: 1, width: 7, height: 7 })
  assert.equal(visibleBounds(pixels, region('empty', 9, 0, 3, 9)), null)
})

test('bottom and centre align artwork with different crop dimensions on a shared canvas', () => {
  const pixels = image(24, 14, [[2, 2, 255], [3, 5, 255], [14, 1, 255], [18, 8, 255]])
  const regions = [region('short', 0, 0, 8, 8), region('tall', 12, 0, 10, 12)]
  const bottom = calculateLayout(pixels, regions, { mode: 'bottom', padding: 3, minWidth: 0, minHeight: 0 })
  assert.equal(bottom.placements.short.y + bottom.placements.short.source.height, bottom.placements.tall.y + bottom.placements.tall.source.height)
  assert.equal(bottom.placements.short.x + Math.floor(bottom.placements.short.source.width / 2), bottom.placements.tall.x + Math.floor(bottom.placements.tall.source.width / 2))
  const centre = calculateLayout(pixels, regions, { mode: 'center', padding: 3, minWidth: 0, minHeight: 0 })
  assert.equal(centre.placements.short.y + Math.floor(centre.placements.short.source.height / 2), centre.placements.tall.y + Math.floor(centre.placements.tall.source.height / 2))
  assert.ok(bottom.width >= 5 + 6 && bottom.height >= 8 + 6)
})

test('manual offsets expand bounds and minimum canvas size adds transparent space', () => {
  const pixels = image(12, 8, [[1, 1, 255], [3, 3, 255], [8, 1, 255], [10, 3, 255]])
  const plain = [region('a', 0, 0, 5, 5), region('b', 7, 0, 5, 5)]
  const shifted = [plain[0], region('b', 7, 0, 5, 5, 12, -7)]
  const options = { mode: 'bottom', padding: 4, minWidth: 0, minHeight: 0 }
  const base = calculateLayout(pixels, plain, options)
  const adjusted = calculateLayout(pixels, shifted, options)
  assert.ok(adjusted.width > base.width && adjusted.height > base.height)
  assert.equal(adjusted.placements.b.x - adjusted.placements.a.x, 12)
  const roomy = calculateLayout(pixels, plain, { ...options, minWidth: 60, minHeight: 70 })
  assert.deepEqual([roomy.width, roomy.height], [60, 70])
  assert.ok(roomy.placements.a.x >= 4 && roomy.placements.a.y >= 4)
})

test('original position preserves deliberate motion inside equally sized source rectangles', () => {
  const pixels = image(16, 8, [[1, 3, 255], [12, 3, 255]])
  const regions = [region('left', 0, 0, 8, 8), region('right', 8, 0, 8, 8)]
  const original = calculateLayout(pixels, regions, { mode: 'original', padding: 2, minWidth: 0, minHeight: 0 })
  const relativeLeft = original.placements.left.x + (1 - regions[0].x)
  const relativeRight = original.placements.right.x + (12 - regions[1].x)
  assert.equal(relativeRight - relativeLeft, 3)
  assert.deepEqual([original.placements.left.source.width, original.placements.right.source.width], [8, 8])
})

test('transparent regions have no placement and cannot draw stale pixels', () => {
  const pixels = image(8, 8, [[1, 1, 255]])
  const layout = calculateLayout(pixels, [region('a', 0, 0, 4, 4), region('blank', 4, 0, 4, 4)], { mode: 'center', padding: 2, minWidth: 0, minHeight: 0 })
  assert.equal(layout.placements.blank, null)
})

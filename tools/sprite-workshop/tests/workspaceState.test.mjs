import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const source = await readFile(new URL('../src/App.tsx', import.meta.url), 'utf8')

test('switching workflow tabs changes view state without resetting workshop data', () => {
  const selectTab = source.match(/function selectTab\(tab: WorkflowTab\) \{([\s\S]*?)\n  \}/)?.[1] ?? ''
  assert.match(selectTab, /setActiveTab\(tab\)/)
  for (const setter of ['setSource', 'setRegions', 'setSuggestions', 'setSlots', 'setOffsets', 'setAlignmentZoom', 'setAlignmentPan', 'setFps', 'setPlaying']) {
    assert.doesNotMatch(selectTab, new RegExp(`${setter}\\(`), `${setter} must not run during tab selection`)
  }
})

test('Animate hides the source editor through layout while retaining the shared preview state', () => {
  assert.match(source, /activeTab === 'animate' \? 'workspace animate-workspace'/)
  assert.match(source, /const \[alignmentZoom, setAlignmentZoom\] = useState/)
  assert.match(source, /const \[alignmentPan, setAlignmentPan\] = useState/)
  assert.match(source, /<SpriteCanvas image=\{source\.bitmap\} placement=\{layout\.placements\[activeSlot\]\}/)
})

// Dev-time generator for tests/fixtures/semver_min.txt — ground truth for
// src/sort/semver_min.rs, taken from node-semver's own minVersion.
// Run with: node scripts/gen_semver_fixture.mjs
import { writeFileSync } from 'node:fs'
import minVersion from 'semver/ranges/min-version.js'

const RANGES = [
  '1', '1.2', '1.2.3', '2', '10', '0', 'v1.2.3', '  1.2.3  ', '=1.2', '1.2.3-0',
  '^1.2.3', '^1', '^1.2', '^0', '^0.2', '^0.0.1', '^1.x', '^1.0.0-beta.1',
  '~1.2', '~1.2.3', '~0', '~>1.2',
  '>=2', '>=2.3.4', '>2', '>2.3', '>2.3.4', '>0', '>1.2.3-beta.1',
  '<3', '<=3', '<2 >=1', '<0.5.0',
  '*', 'x', 'X', '', '1.x', '1.2.x', '1.x.x', '1.*',
  '1.2.3 - 2.0.0', '1.2 - 2', '1 - 2',
  '1 || 2', '2 || 1', '>=1.0.0 <2.0.0', '>=1.2.3 <2 || >=3',
  '1.0.0-alpha', '1.0.0-alpha.1', '1.0.0-alpha.beta', '1.0.0-rc.1', '1.0.0-0',
]

const lines = RANGES.map((range) => {
  let out
  try {
    const v = minVersion(range)
    out = v === null ? 'NONE' : v.version
  } catch {
    out = 'THROW'
  }
  return `${range}\t${out}`
})
writeFileSync(new URL('../tests/fixtures/semver_min.txt', import.meta.url), lines.join('\n') + '\n')
console.log(`wrote ${lines.length} ranges`)

import assert from 'node:assert/strict'
import test from 'node:test'

import { checkFileCoverage } from './check-coverage.mjs'

void test('accepts files whose four metrics meet the per-file minimum', () => {
  assert.equal(
    checkFileCoverage({
      total: report(90),
      'src/example.ts': report(60),
      'src/other.ts': report(100),
    }),
    2,
  )
})

void test('reports every metric below the per-file minimum', () => {
  assert.throws(
    () =>
      checkFileCoverage({
        total: report(90),
        'src/example.ts': {
          branches: { pct: 59.99 },
          functions: { pct: 80 },
          lines: { pct: 50 },
          statements: { pct: 75 },
        },
      }),
    /src\/example\.ts: branches 59\.99%[\s\S]*src\/example\.ts: lines 50%/,
  )
})

void test('rejects malformed or empty summaries', () => {
  assert.throws(() => checkFileCoverage(null), /JSON object/)
  assert.throws(() => checkFileCoverage({ total: report(100) }), /does not contain any files/)
  assert.throws(
    () => checkFileCoverage({ 'src/example.ts': { ...report(100), lines: {} } }),
    /missing lines\.pct/,
  )
})

function report(pct) {
  return {
    branches: { pct },
    functions: { pct },
    lines: { pct },
    statements: { pct },
  }
}

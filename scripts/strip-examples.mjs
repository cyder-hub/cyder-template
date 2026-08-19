#!/usr/bin/env node

import { runStripExamplesCli } from './template-project.mjs'

try {
  await runStripExamplesCli(process.argv.slice(2))
} catch (error) {
  console.error(`example cleanup failed: ${error instanceof Error ? error.message : error}`)
  process.exitCode = 1
}

#!/usr/bin/env node

import { runInitCli } from './template-project.mjs'

try {
  await runInitCli(process.argv.slice(2))
} catch (error) {
  console.error(`project initialization failed: ${error instanceof Error ? error.message : error}`)
  process.exitCode = 1
}

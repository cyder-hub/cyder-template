#!/usr/bin/env node

import { writeSync } from 'node:fs'

import { runInitCli } from './template-project.mjs'

try {
  await runInitCli(process.argv.slice(2))
} catch (error) {
  writeSync(
    process.stderr.fd,
    `project initialization failed: ${error instanceof Error ? error.message : error}\n`,
  )
  process.exitCode = 1
}

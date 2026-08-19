#!/usr/bin/env node

import { resolveDevProxyTarget } from '../front/dev-proxy.mts'

try {
  process.stdout.write(`${resolveDevProxyTarget()}\n`)
} catch (error) {
  const message = error instanceof Error ? error.message : String(error)
  process.stderr.write(`[dev-proxy] ${message}\n`)
  process.exitCode = 1
}

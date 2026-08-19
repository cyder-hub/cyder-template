import assert from 'node:assert/strict'
import { createServer } from 'node:net'
import { once } from 'node:events'
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import {
  PROJECT_ROOT,
  compareVersions,
  configurationProblems,
  diagnose,
  loadRequirements,
  parsePort,
  parseVersion,
  probePort,
} from './doctor.mjs'
import {
  bootstrap,
  prepareLocalEnvironment,
} from './bootstrap.mjs'

test('parses and compares semantic tool versions', () => {
  assert.deepEqual(parseVersion('rustc 1.97.1 (build metadata)'), [1, 97, 1])
  assert.deepEqual(parseVersion('v24.19.0'), [24, 19, 0])
  assert.equal(parseVersion('not-a-version'), undefined)
  assert.equal(compareVersions('1.58.0', '1.58.0'), 0)
  assert.equal(compareVersions('1.58.1', '1.58.0'), 1)
  assert.equal(compareVersions('1.57.9', '1.58.0'), -1)
  assert.equal(parsePort('8000'), 8000)
  assert.equal(parsePort('0'), undefined)
  assert.equal(parsePort('not-a-port'), undefined)
})

test('keeps checked-in Node and Rust declarations aligned', () => {
  const requirements = loadRequirements()
  assert.deepEqual(requirements, {
    nodeEngine: '>=24.19.0 <25',
    nodeVersion: '24.19.0',
    npmEngine: '>=11.17.0 <12',
    npmVersion: '11.17.0',
    rustVersion: '1.97.1',
  })
  assert.deepEqual(configurationProblems(), [])

  const workflows = [
    '.github/workflows/ci.yml',
    '.github/workflows/security.yml',
    '.github/workflows/template-integration.yml',
  ]
    .map((path) => join(PROJECT_ROOT, path))
    .filter(existsSync)
    .map((path) => readFileSync(path, 'utf8'))
    .join('\n')
  const setupNodeCount = workflows.match(/uses: actions\/setup-node@/g)?.length ?? 0
  const versionFileCount = workflows.match(/node-version-file: \.node-version/g)?.length ?? 0
  assert.ok(setupNodeCount > 0)
  assert.equal(versionFileCount, setupNodeCount)
  assert.equal(/node-version:\s*24(?:\s|$)/m.test(workflows), false)

  const securityWorkflow = readFileSync(
    join(PROJECT_ROOT, '.github/workflows/security.yml'),
    'utf8',
  )
  assert.equal(securityWorkflow.includes(`rust-version: "${requirements.rustVersion}"`), true)
})

test('diagnoses the required path without requiring Docker', async () => {
  const command = (name, args) => {
    const key = [name, ...args].join(' ')
    const outputs = new Map([
      ['npm --version', '11.17.0'],
      ['rustup toolchain list', '1.97.1-x86_64-unknown-linux-gnu'],
      ['rustup run 1.97.1 rustc --version', 'rustc 1.97.1 (test)'],
      ['rustup run 1.97.1 cargo --version', 'cargo 1.97.1 (test)'],
      ['rustup run 1.97.1 cargo fmt --version', 'rustfmt 1.8.0-stable (test)'],
      ['rustup run 1.97.1 cargo clippy --version', 'clippy 0.1.97 (test)'],
      ['just --version', 'just 1.58.0'],
    ])
    return outputs.has(key)
      ? { ok: true, output: outputs.get(key) }
      : { ok: false, output: '', error: `${name} unavailable` }
  }
  const results = await diagnose({
    command,
    env: {},
    nodeVersion: '24.19.0',
    probe: async () => ({ available: true }),
  })
  assert.equal(results.some((item) => item.level === 'fail'), false)
  assert.equal(
    results.some((item) => item.level === 'warn' && item.label === 'Docker'),
    true,
  )
})

test('reports a missing pinned Rust toolchain without invoking an installer', async () => {
  const commands = []
  const command = (name, args) => {
    const key = [name, ...args].join(' ')
    commands.push(key)
    const outputs = {
      'npm --version': '11.17.0',
      'rustup toolchain list': 'stable-x86_64-unknown-linux-gnu',
      'just --version': 'just 1.58.0',
    }
    return key in outputs
      ? { ok: true, output: outputs[key] }
      : { ok: false, output: '', error: `${name} unavailable` }
  }
  const results = await diagnose({
    command,
    env: {},
    nodeVersion: '24.19.0',
    probe: async () => ({ available: true }),
  })
  assert.equal(
    results.some(
      (item) => item.level === 'fail' && item.label === 'Rust' && item.detail.includes('is not installed'),
    ),
    true,
  )
  assert.equal(commands.some((entry) => entry.startsWith('rustup run ')), false)
  assert.equal(commands.some((entry) => entry.includes(' install ')), false)
})

test('treats the backend port as blocking and optional ports as warnings', async () => {
  const command = (name, args) => {
    const key = [name, ...args].join(' ')
    const outputs = {
      'npm --version': '11.17.0',
      'rustup toolchain list': '1.97.1-x86_64-unknown-linux-gnu',
      'rustup run 1.97.1 rustc --version': 'rustc 1.97.1 (test)',
      'rustup run 1.97.1 cargo --version': 'cargo 1.97.1 (test)',
      'rustup run 1.97.1 cargo fmt --version': 'rustfmt 1.8.0-stable (test)',
      'rustup run 1.97.1 cargo clippy --version': 'clippy 0.1.97 (test)',
      'just --version': 'just 1.58.0',
      'docker --version': 'Docker version 29.0.0',
      'docker compose version': 'Docker Compose version v2.40.0',
      'docker info --format {{.ServerVersion}}': '29.0.0',
    }
    return key in outputs ? { ok: true, output: outputs[key] } : { ok: false, output: '' }
  }
  const results = await diagnose({
    command,
    env: { APP_PORT: '8000', POSTGRES_PORT: '5432' },
    nodeVersion: '24.19.0',
    probe: async (port) => ({ available: ![8000, 5173, 5432].includes(port) }),
  })
  assert.equal(
    results.some((item) => item.level === 'fail' && item.label === 'Backend port'),
    true,
  )
  assert.equal(
    results.filter((item) => item.level === 'warn' && item.label.endsWith('port')).length,
    2,
  )
})

test('detects a port already bound by another process', async () => {
  const server = createServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  try {
    const address = server.address()
    const status = await probePort(address.port, '127.0.0.1')
    assert.equal(status.available, false)
    assert.equal(status.error.code, 'EADDRINUSE')
  } finally {
    server.close()
    await once(server, 'close')
  }
})

test('prepares local files idempotently without overwriting .env', () => {
  const root = mkdtempSync(join(tmpdir(), 'toolchain-bootstrap-test-'))
  try {
    mkdirSync(join(root, 'front'), { recursive: true })
    writeFileSync(join(root, '.env.example'), 'APP_PORT=8000\n')
    const commands = []
    assert.throws(
      () => bootstrap(root, () => ({ status: 23 })),
      /npm ci failed with status 23/,
    )
    assert.equal(existsSync(join(root, '.env')), false)

    const runner = (name, args, options) => {
      commands.push({ name, args, cwd: options.cwd })
      return { status: 0 }
    }

    const first = bootstrap(root, runner)
    assert.equal(first.environmentAction, 'created')
    assert.equal(existsSync(first.dataDirectory), true)
    assert.equal(readFileSync(join(root, '.env'), 'utf8'), 'APP_PORT=8000\n')
    assert.deepEqual(commands, [
      { name: 'npm', args: ['--prefix', 'front', 'ci'], cwd: root },
    ])

    writeFileSync(join(root, '.env'), 'APP_PORT=9000\n')
    const second = prepareLocalEnvironment(root)
    assert.equal(second.environmentAction, 'preserved')
    assert.equal(readFileSync(join(root, '.env'), 'utf8'), 'APP_PORT=9000\n')
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

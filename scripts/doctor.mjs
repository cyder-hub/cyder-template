import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const PROJECT_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const MINIMUM_JUST_VERSION = '1.58.0'
export const VITE_DEFAULT_PORT = 5173

export function parseVersion(value) {
  const match = String(value).match(/(?:^|\s|v)(\d+)\.(\d+)\.(\d+)(?:\b|$)/)
  return match ? match.slice(1, 4).map(Number) : undefined
}

export function compareVersions(left, right) {
  const leftParts = Array.isArray(left) ? left : parseVersion(left)
  const rightParts = Array.isArray(right) ? right : parseVersion(right)
  if (!leftParts || !rightParts) {
    throw new Error(`unable to compare versions '${left}' and '${right}'`)
  }
  for (let index = 0; index < 3; index += 1) {
    if (leftParts[index] !== rightParts[index]) {
      return leftParts[index] < rightParts[index] ? -1 : 1
    }
  }
  return 0
}

export function parsePort(value) {
  if (!/^\d+$/.test(String(value))) {
    return undefined
  }
  const port = Number(value)
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : undefined
}

function requiredTomlValue(text, key, path) {
  const match = text.match(new RegExp(`^${key}\\s*=\\s*"([^"]+)"`, 'm'))
  if (!match) {
    throw new Error(`missing ${key} in ${path}`)
  }
  return match[1]
}

export function loadRequirements(root = PROJECT_ROOT) {
  const nodeVersion = readFileSync(join(root, '.node-version'), 'utf8').trim()
  const frontendPackage = JSON.parse(readFileSync(join(root, 'front/package.json'), 'utf8'))
  const packageManager = /^npm@(.+)$/.exec(frontendPackage.packageManager ?? '')
  if (!packageManager) {
    throw new Error('front/package.json must declare packageManager as npm@<version>')
  }

  const rustToolchain = readFileSync(join(root, 'rust-toolchain.toml'), 'utf8')
  const rustVersion = requiredTomlValue(rustToolchain, 'channel', 'rust-toolchain.toml')

  return {
    nodeEngine: frontendPackage.engines?.node,
    nodeVersion,
    npmEngine: frontendPackage.engines?.npm,
    npmVersion: packageManager[1],
    rustVersion,
  }
}

export function configurationProblems(root = PROJECT_ROOT) {
  const requirements = loadRequirements(root)
  const dockerfile = readFileSync(join(root, 'Dockerfile'), 'utf8')
  const cargoManifest = readFileSync(join(root, 'server/Cargo.toml'), 'utf8')
  const frontendLock = JSON.parse(readFileSync(join(root, 'front/package-lock.json'), 'utf8'))
  const lockedEngines = frontendLock.packages?.['']?.engines
  const problems = []

  const expectedNodeEngine = `>=${requirements.nodeVersion} <${parseVersion(requirements.nodeVersion)[0] + 1}`
  if (requirements.nodeEngine !== expectedNodeEngine) {
    problems.push(`front/package.json engines.node must be '${expectedNodeEngine}'`)
  }
  const expectedNpmEngine = `>=${requirements.npmVersion} <${parseVersion(requirements.npmVersion)[0] + 1}`
  if (requirements.npmEngine !== expectedNpmEngine) {
    problems.push(`front/package.json engines.npm must be '${expectedNpmEngine}'`)
  }
  if (lockedEngines?.node !== expectedNodeEngine || lockedEngines?.npm !== expectedNpmEngine) {
    problems.push('front/package-lock.json engines must match front/package.json')
  }
  if (!dockerfile.includes(`ARG NODE_VERSION=${requirements.nodeVersion}`)) {
    problems.push('Dockerfile NODE_VERSION must match .node-version')
  }
  if (!dockerfile.includes('FROM node:${NODE_VERSION}-bookworm-slim')) {
    problems.push('Dockerfile frontend stage must consume NODE_VERSION')
  }
  if (!dockerfile.includes(`ARG RUST_VERSION=${requirements.rustVersion}`)) {
    problems.push('Dockerfile RUST_VERSION must match rust-toolchain.toml')
  }
  if (!dockerfile.includes('FROM rust:${RUST_VERSION}-bookworm')) {
    problems.push('Dockerfile backend stage must consume RUST_VERSION')
  }

  const cargoRustVersion = requiredTomlValue(cargoManifest, 'rust-version', 'server/Cargo.toml')
  if (cargoRustVersion !== requirements.rustVersion) {
    problems.push('server/Cargo.toml rust-version must match the pinned Rust version')
  }
  return problems
}

export function runCommand(command, args = [], options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? PROJECT_ROOT,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: options.timeoutMs ?? 10_000,
  })
  if (result.error) {
    return { ok: false, output: '', error: result.error.message }
  }
  const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim()
  return { ok: result.status === 0, output, status: result.status }
}

export function probePort(port, host = '127.0.0.1') {
  return new Promise((resolveProbe) => {
    const server = createServer()
    let settled = false
    const finish = (status) => {
      if (settled) {
        return
      }
      settled = true
      clearTimeout(timeout)
      resolveProbe(status)
    }
    const timeout = setTimeout(() => {
      try {
        server.close()
      } catch {
        // The socket may still be resolving and not yet be open.
      }
      const error = new Error(`port probe timed out for ${host}:${port}`)
      error.code = 'ETIMEDOUT'
      finish({ available: false, error })
    }, 2_000)
    server.unref()
    server.once('error', (error) => finish({ available: false, error }))
    server.listen({ host, port, exclusive: true }, () => {
      server.close((error) => finish(error ? { available: false, error } : { available: true }))
    })
  })
}

function result(level, label, detail, fix) {
  return { level, label, detail, fix }
}

function exactVersionResult(label, actualOutput, expected, fix) {
  const actual = parseVersion(actualOutput)
  if (!actual) {
    return result('fail', label, `unable to parse version from '${actualOutput}'`, fix)
  }
  if (compareVersions(actual, expected) !== 0) {
    return result('fail', label, `found ${actual.join('.')}; required ${expected}`, fix)
  }
  return result('ok', label, actual.join('.'))
}

async function portResult({ label, value, host, level, occupiedDetail, fix, probe }) {
  const port = parsePort(value)
  if (!port) {
    return result(level, label, `invalid port '${value}'`, fix)
  }
  const status = await probe(port, host)
  if (!status.available) {
    return result(level, label, occupiedDetail(port, host, status.error), fix)
  }
  return result('ok', label, `${host}:${port} is available`)
}

export async function diagnose(options = {}) {
  const root = options.root ?? PROJECT_ROOT
  const env = options.env ?? process.env
  const command = options.command ?? ((name, args, commandOptions = {}) => runCommand(name, args, {
    cwd: commandOptions.cwd ?? root,
    timeoutMs: commandOptions.timeoutMs,
  }))
  const probe = options.probe ?? probePort
  const nodeVersion = options.nodeVersion ?? process.versions.node
  const requirements = loadRequirements(root)
  const results = []

  for (const problem of configurationProblems(root)) {
    results.push(result('fail', 'Toolchain metadata', problem, 'Synchronize the checked-in version declarations.'))
  }

  results.push(
    exactVersionResult(
      'Node.js',
      nodeVersion,
      requirements.nodeVersion,
      `Install Node.js ${requirements.nodeVersion} from the version in .node-version and ensure it is first on PATH.`,
    ),
  )

  const npm = command('npm', ['--version'])
  results.push(
    npm.ok
      ? exactVersionResult(
          'npm',
          npm.output,
          requirements.npmVersion,
          `Install npm ${requirements.npmVersion} or use the npm bundled with Node.js ${requirements.nodeVersion}.`,
        )
      : result('fail', 'npm', (npm.error ?? npm.output) || 'command failed', `Install Node.js ${requirements.nodeVersion}, which includes npm.`),
  )

  const rustInstallCommand = `rustup toolchain install ${requirements.rustVersion} --profile minimal --component rustfmt --component clippy`
  const installedToolchains = command('rustup', ['toolchain', 'list'], {
    cwd: tmpdir(),
    timeoutMs: 3_000,
  })
  const escapedRustVersion = requirements.rustVersion.replaceAll('.', '\\.')
  const pinnedToolchainInstalled = installedToolchains.ok && new RegExp(
    `^${escapedRustVersion}(?:-|\\s|$)`,
    'm',
  ).test(installedToolchains.output)
  if (!pinnedToolchainInstalled) {
    results.push(
      result(
        'fail',
        'Rust',
        installedToolchains.ok
          ? `toolchain ${requirements.rustVersion} is not installed`
          : (installedToolchains.error ?? installedToolchains.output) || 'rustup is unavailable',
        `Run: ${rustInstallCommand}`,
      ),
    )
  } else {
    const rustc = command('rustup', ['run', requirements.rustVersion, 'rustc', '--version'])
    results.push(
      rustc.ok
        ? exactVersionResult('Rust', rustc.output, requirements.rustVersion, `Run: ${rustInstallCommand}`)
        : result('fail', 'Rust', (rustc.error ?? rustc.output) || 'command failed', `Run: ${rustInstallCommand}`),
    )

    for (const [label, args, fix] of [
      ['Cargo', ['run', requirements.rustVersion, 'cargo', '--version'], 'Install Cargo through rustup.'],
      ['rustfmt', ['run', requirements.rustVersion, 'cargo', 'fmt', '--version'], `Run: rustup component add rustfmt --toolchain ${requirements.rustVersion}`],
      ['Clippy', ['run', requirements.rustVersion, 'cargo', 'clippy', '--version'], `Run: rustup component add clippy --toolchain ${requirements.rustVersion}`],
    ]) {
      const checked = command('rustup', args)
      results.push(checked.ok ? result('ok', label, checked.output.split('\n')[0]) : result('fail', label, (checked.error ?? checked.output) || 'command failed', fix))
    }
  }

  const just = command('just', ['--version'])
  if (!just.ok) {
    results.push(result('fail', 'just', (just.error ?? just.output) || 'command failed', `Install just ${MINIMUM_JUST_VERSION} or a newer 1.x release.`))
  } else {
    const version = parseVersion(just.output)
    if (!version || compareVersions(version, MINIMUM_JUST_VERSION) < 0 || version[0] >= 2) {
      results.push(result('fail', 'just', `found ${just.output}; required >=${MINIMUM_JUST_VERSION} <2`, `Install just ${MINIMUM_JUST_VERSION} or a newer 1.x release.`))
    } else {
      results.push(result('ok', 'just', version.join('.')))
    }
  }

  const docker = command('docker', ['--version'])
  if (!docker.ok) {
    results.push(result('warn', 'Docker', (docker.error ?? docker.output) || 'command not found', 'Install Docker only if you need container builds or PostgreSQL Compose.'))
  } else {
    results.push(result('ok', 'Docker', docker.output.split('\n')[0]))
    const compose = command('docker', ['compose', 'version'])
    const composeVersion = compose.ok ? parseVersion(compose.output) : undefined
    const supportedCompose = composeVersion && composeVersion[0] >= 2
    results.push(
      supportedCompose
        ? result('ok', 'Docker Compose', compose.output.split('\n')[0])
        : result('warn', 'Docker Compose', compose.output || 'Compose plugin version 2 or newer is unavailable', 'Install the Docker Compose plugin version 2 or newer for container workflows.'),
    )
    if (supportedCompose) {
      const daemon = command('docker', ['info', '--format', '{{.ServerVersion}}'], { timeoutMs: 3_000 })
      results.push(
        daemon.ok
          ? result('ok', 'Docker daemon', daemon.output)
          : result('warn', 'Docker daemon', daemon.output || 'daemon is unavailable', 'Start Docker before using container workflows.'),
      )
    }
  }

  const backendHost = env.APP_HOST || '127.0.0.1'
  results.push(
    await portResult({
      label: 'Backend port',
      value: env.APP_PORT || '8000',
      host: backendHost,
      level: 'fail',
      occupiedDetail: (port, host) => `${host}:${port} is unavailable`,
      fix: 'Stop the process using the backend port or set APP_PORT to an available port.',
      probe,
    }),
  )
  results.push(
    await portResult({
      label: 'Vite port',
      value: String(VITE_DEFAULT_PORT),
      host: '127.0.0.1',
      level: 'warn',
      occupiedDetail: (port) => `127.0.0.1:${port} is occupied; Vite will select another port`,
      fix: 'No action is required; use the URL printed by Vite.',
      probe,
    }),
  )
  results.push(
    await portResult({
      label: 'PostgreSQL port',
      value: '5432',
      host: '127.0.0.1',
      level: 'warn',
      occupiedDetail: (port) => `127.0.0.1:${port} is occupied; local PostgreSQL Compose may not start`,
      fix: 'Stop the conflicting service or use a Docker Compose override file.',
      probe,
    }),
  )

  return results
}

export function printResults(results) {
  console.log('Development environment doctor\n')
  for (const item of results) {
    console.log(`[${item.level}] ${item.label}: ${item.detail}`)
    if (item.fix && item.level !== 'ok') {
      console.log(`       ${item.fix}`)
    }
  }
  const failures = results.filter((item) => item.level === 'fail').length
  const warnings = results.filter((item) => item.level === 'warn').length
  console.log(`\nDoctor completed with ${failures} blocking issue(s) and ${warnings} warning(s).`)
  return failures === 0
}

function isMainModule() {
  return process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)
}

if (isMainModule()) {
  try {
    const healthy = printResults(await diagnose())
    if (!healthy) {
      process.exitCode = 1
    }
  } catch (error) {
    console.error(`[fail] doctor: ${error.message}`)
    process.exitCode = 1
  }
}

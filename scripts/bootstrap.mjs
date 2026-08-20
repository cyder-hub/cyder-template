import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join, resolve } from 'node:path'

export const PROJECT_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')

export function installLockedFrontendDependencies(root = PROJECT_ROOT, runner = spawnSync) {
  const result = runner('npm', ['--prefix', 'front', 'ci'], {
    cwd: root,
    stdio: 'inherit',
  })
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    throw new Error(`npm ci failed with status ${result.status ?? 'unknown'}`)
  }
}

export function prepareLocalEnvironment(root = PROJECT_ROOT) {
  const dataDirectory = join(root, '.app', 'dev')
  for (const child of ['config', 'db', 'storage', 'logs']) {
    mkdirSync(join(dataDirectory, child), { recursive: true })
  }
  return { dataDirectory }
}

export function bootstrap(root = PROJECT_ROOT, runner = spawnSync) {
  installLockedFrontendDependencies(root, runner)
  return prepareLocalEnvironment(root)
}

function isMainModule() {
  return process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)
}

if (isMainModule()) {
  try {
    const result = bootstrap()
    console.log(`Prepared local data directory: ${result.dataDirectory}`)
    console.log('Bootstrap complete. Run `just dev` to start the application.')
  } catch (error) {
    console.error(`bootstrap failed: ${error.message}`)
    process.exitCode = 1
  }
}

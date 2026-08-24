import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

export const DEV_PROXY_TARGET_ENV = 'DEV_PROXY_TARGET'

const PROJECT_ROOT = fileURLToPath(new URL('..', import.meta.url))

export interface ConfigEndpoint {
  host: string
  port: number
}

interface ResolveOptions {
  env?: NodeJS.ProcessEnv
  queryEndpoint?: () => string
}

export function validateProxyTarget(value: string): string {
  if (value.length === 0 || value.trim().length === 0) {
    throw new Error(`${DEV_PROXY_TARGET_ENV} is set but empty`)
  }
  if (value !== value.trim()) {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must not contain leading or trailing whitespace`)
  }

  let url: URL
  try {
    url = new URL(value)
  } catch {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must be an absolute HTTP(S) origin`)
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must use http:// or https://`)
  }
  if (url.username || url.password) {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must not contain credentials`)
  }
  if (!url.hostname) {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must contain a host`)
  }
  if (url.pathname !== '/' || url.search || url.hash) {
    throw new Error(`${DEV_PROXY_TARGET_ENV} must be an origin without a path, query, or fragment`)
  }

  return url.origin
}

export function parseConfigEndpoint(output: string): ConfigEndpoint {
  let value: unknown
  try {
    value = JSON.parse(output)
  } catch {
    throw new Error('Rust config endpoint returned invalid JSON')
  }

  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Rust config endpoint must return a JSON object')
  }

  const endpoint = value as Record<string, unknown>
  if (typeof endpoint.host !== 'string' || endpoint.host.length === 0) {
    throw new Error('Rust config endpoint returned an invalid host')
  }
  if (
    !Number.isInteger(endpoint.port) ||
    (endpoint.port as number) < 0 ||
    (endpoint.port as number) > 65_535
  ) {
    throw new Error('Rust config endpoint returned an invalid port')
  }

  return {
    host: endpoint.host,
    port: endpoint.port as number,
  }
}

export function proxyTargetFromEndpoint(endpoint: ConfigEndpoint): string {
  if (endpoint.port === 0) {
    throw new Error(
      'cannot derive a development proxy target from backend port 0; configure a fixed port',
    )
  }

  let connectHost = endpoint.host
  if (connectHost === '0.0.0.0') {
    connectHost = '127.0.0.1'
  } else if (connectHost === '::') {
    connectHost = '::1'
  }

  const urlHost = connectHost.includes(':') ? `[${connectHost}]` : connectHost
  return validateProxyTarget(`http://${urlHost}:${String(endpoint.port)}`)
}

export function queryRustConfigEndpoint(): string {
  const result = spawnSync(
    'cargo',
    [
      'run',
      '--quiet',
      '--locked',
      '-p',
      'cyder-template',
      '--',
      'config',
      'endpoint',
      '--format',
      'json',
    ],
    {
      cwd: PROJECT_ROOT,
      encoding: 'utf8',
      env: process.env,
      maxBuffer: 1024 * 1024,
    },
  )

  if (result.error) {
    throw new Error(`failed to run the Rust config endpoint: ${result.error.message}`)
  }
  if (result.status !== 0) {
    const detail =
      result.stderr.trim() ||
      `process exited with status ${result.status === null ? 'unknown' : String(result.status)}`
    throw new Error(`Rust config endpoint failed: ${detail}`)
  }

  return result.stdout
}

export function resolveDevProxyTarget(options: ResolveOptions = {}): string {
  const env = options.env ?? process.env
  if (Object.hasOwn(env, DEV_PROXY_TARGET_ENV)) {
    const explicitTarget = env[DEV_PROXY_TARGET_ENV]
    if (explicitTarget === undefined) {
      throw new Error(`${DEV_PROXY_TARGET_ENV} is set but has no value`)
    }
    return validateProxyTarget(explicitTarget)
  }

  const output = (options.queryEndpoint ?? queryRustConfigEndpoint)()
  return proxyTargetFromEndpoint(parseConfigEndpoint(output))
}

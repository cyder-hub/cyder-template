import assert from 'node:assert/strict'
import { createServer as createHttpServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { createServer as createViteServer } from 'vite'
import type { ViteDevServer } from 'vite'

import {
  DEV_PROXY_TARGET_ENV,
  parseConfigEndpoint,
  proxyTargetFromEndpoint,
  resolveDevProxyTarget,
  validateProxyTarget,
} from './dev-proxy.mts'
import { createViteConfig } from './vite.config.ts'

const developmentConfigEnvironment = {
  command: 'serve' as const,
  mode: 'development',
  isSsrBuild: false,
  isPreview: false,
}

void test('validates and canonicalizes explicit HTTP(S) origins', () => {
  assert.equal(validateProxyTarget('https://dev-api.example.com/'), 'https://dev-api.example.com')
  assert.equal(validateProxyTarget('http://[::1]:9000'), 'http://[::1]:9000')

  for (const target of [
    '',
    ' https://dev-api.example.com',
    'ftp://dev-api.example.com',
    'https://user:secret@dev-api.example.com',
    'https://dev-api.example.com/backend',
    'https://dev-api.example.com?mode=dev',
    'https://dev-api.example.com#fragment',
  ]) {
    assert.throws(() => validateProxyTarget(target), DEV_PROXY_TARGET_ENV)
  }
})

void test('converts resolved wildcard listeners and rejects port zero', () => {
  assert.equal(proxyTargetFromEndpoint({ host: '0.0.0.0', port: 19041 }), 'http://127.0.0.1:19041')
  assert.equal(proxyTargetFromEndpoint({ host: '::', port: 19042 }), 'http://[::1]:19042')
  assert.equal(
    proxyTargetFromEndpoint({ host: '192.0.2.10', port: 19043 }),
    'http://192.0.2.10:19043',
  )
  assert.throws(() => proxyTargetFromEndpoint({ host: '127.0.0.1', port: 0 }), /port 0/)
})

void test('validates the Rust endpoint JSON contract', () => {
  assert.deepEqual(parseConfigEndpoint('{"host":"127.0.0.1","port":9000}'), {
    host: '127.0.0.1',
    port: 9000,
  })
  assert.throws(() => parseConfigEndpoint('not json'), /invalid JSON/)
  assert.throws(() => parseConfigEndpoint('{"host":"127.0.0.1","port":70000}'), /invalid port/)
})

void test('an explicit target bypasses Rust configuration lookup', () => {
  let queried = false
  assert.equal(
    resolveDevProxyTarget({
      env: { [DEV_PROXY_TARGET_ENV]: 'https://dev-api.example.com' },
      queryEndpoint: () => {
        queried = true
        return '{}'
      },
    }),
    'https://dev-api.example.com',
  )
  assert.equal(queried, false)

  assert.throws(
    () =>
      resolveDevProxyTarget({
        env: { [DEV_PROXY_TARGET_ENV]: '' },
        queryEndpoint: () => {
          queried = true
          return '{}'
        },
      }),
    /set but empty/,
  )
  assert.equal(queried, false)
})

void test('an absent target is derived from the Rust endpoint JSON', () => {
  assert.equal(
    resolveDevProxyTarget({
      env: {},
      queryEndpoint: () => '{"host":"0.0.0.0","port":19044}',
    }),
    'http://127.0.0.1:19044',
  )
})

void test('the Vite development server requires an injected target', () => {
  const previousTarget = process.env[DEV_PROXY_TARGET_ENV]
  delete process.env.DEV_PROXY_TARGET
  try {
    assert.throws(
      () => createViteConfig(developmentConfigEnvironment),
      new RegExp(`${DEV_PROXY_TARGET_ENV} is required`),
    )
    const buildConfig = createViteConfig({
      ...developmentConfigEnvironment,
      command: 'build',
    })
    assert.equal(buildConfig.envDir, false)
    assert.doesNotThrow(() =>
      createViteConfig({
        ...developmentConfigEnvironment,
        isPreview: true,
      }),
    )
  } finally {
    if (previousTarget === undefined) {
      delete process.env.DEV_PROXY_TARGET
    } else {
      process.env[DEV_PROXY_TARGET_ENV] = previousTarget
    }
  }
})

void test('Vite proxies every development route to a non-default port', async () => {
  const upstream = createHttpServer((request, response) => {
    response.setHeader('content-type', 'application/json')
    response.end(
      JSON.stringify({
        host: request.headers.host,
        path: request.url,
      }),
    )
  })
  await new Promise<void>((resolve, reject) => {
    upstream.once('error', reject)
    upstream.listen(0, '127.0.0.1', resolve)
  })
  const upstreamAddress = upstream.address() as AddressInfo
  const previousTarget = process.env[DEV_PROXY_TARGET_ENV]
  process.env[DEV_PROXY_TARGET_ENV] = `http://127.0.0.1:${String(upstreamAddress.port)}`
  let vite: ViteDevServer | undefined

  try {
    vite = await createViteServer({
      configFile: fileURLToPath(new URL('./vite.config.ts', import.meta.url)),
      root: fileURLToPath(new URL('.', import.meta.url)),
      logLevel: 'silent',
      server: {
        host: '127.0.0.1',
        port: 0,
        strictPort: true,
      },
    })
    await vite.listen()
    const viteAddress = vite.httpServer?.address() as AddressInfo
    for (const path of ['/api/probe', '/healthz', '/readyz']) {
      const response = await fetch(`http://127.0.0.1:${String(viteAddress.port)}${path}`)
      assert.equal(response.status, 200)
      assert.deepEqual(await response.json(), {
        host: `127.0.0.1:${String(upstreamAddress.port)}`,
        path,
      })
    }
  } finally {
    try {
      const closeUpstream = new Promise<void>((resolve, reject) => {
        upstream.close((error) => {
          if (error) {
            reject(error)
          } else {
            resolve()
          }
        })
      })
      await Promise.all([vite?.close() ?? Promise.resolve(), closeUpstream])
    } finally {
      if (previousTarget === undefined) {
        delete process.env.DEV_PROXY_TARGET
      } else {
        process.env[DEV_PROXY_TARGET_ENV] = previousTarget
      }
    }
  }
})

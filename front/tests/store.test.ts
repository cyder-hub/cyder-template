import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ApiError, requestJson } from '@/services/http'
import { useAppStore, type HealthResponse, type ReadyResponse } from '@/store'

vi.mock('@/services/http', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/services/http')>()
  return { ...original, requestJson: vi.fn() }
})

const health: HealthResponse = { service: 'example-service', status: 'ok' }
const readiness: ReadyResponse = {
  ...health,
  database: { connected: true, kind: 'sqlite' },
  status: 'ready',
}
const requestJsonMock = vi.mocked(requestJson)

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('app store', () => {
  it('exposes loading state and commits a successful status refresh atomically', async () => {
    const healthRequest = deferred<HealthResponse>()
    const readinessRequest = deferred<ReadyResponse>()
    requestJsonMock
      .mockReturnValueOnce(healthRequest.promise)
      .mockReturnValueOnce(readinessRequest.promise)
    const store = useAppStore()

    const refresh = store.refreshStatus()
    expect(store.loading).toBe(true)
    expect(store.error).toBeNull()
    expect(store.isReady).toBe(false)

    healthRequest.resolve(health)
    readinessRequest.resolve(readiness)
    await refresh

    expect(requestJsonMock).toHaveBeenNthCalledWith(1, '/healthz')
    expect(requestJsonMock).toHaveBeenNthCalledWith(2, '/readyz')
    expect(store.loading).toBe(false)
    expect(store.serviceName).toBe('example-service')
    expect(store.health).toEqual(health)
    expect(store.readiness).toEqual(readiness)
    expect(store.isReady).toBe(true)
  })

  it('reports operational errors and keeps the previously committed status', async () => {
    requestJsonMock.mockResolvedValueOnce(health).mockResolvedValueOnce(readiness)
    const store = useAppStore()
    await store.refreshStatus()

    requestJsonMock
      .mockRejectedValueOnce(
        new ApiError(503, {
          error: 'readiness_failed',
          message: 'service is not ready',
          request_id: 'ready-request-id',
        }),
      )
      .mockResolvedValueOnce(readiness)
    await store.refreshStatus()

    expect(store.loading).toBe(false)
    expect(store.error).toBe('service is not ready (Reference: ready-request-id)')
    expect(store.health).toEqual(health)
    expect(store.readiness).toEqual(readiness)
  })
})

function deferred<T>() {
  let resolvePromise!: (value: T) => void
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve
  })
  return { promise, resolve: resolvePromise }
}

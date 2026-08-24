import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import Dashboard from '@/pages/Dashboard.vue'
import { ApiError, requestJson } from '@/services/http'
import type { HealthResponse, ReadyResponse } from '@/store'

vi.mock('@/services/http', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/services/http')>()
  return { ...original, requestJson: vi.fn() }
})

const health: HealthResponse = { service: 'dashboard-service', status: 'ok' }
const readiness: ReadyResponse = {
  ...health,
  database: { connected: true, kind: 'sqlite' },
  status: 'ready',
}
const requestJsonMock = vi.mocked(requestJson)

describe('Dashboard', () => {
  it('shows loading and ready states and supports an explicit refresh', async () => {
    const healthRequest = deferred<HealthResponse>()
    const readinessRequest = deferred<ReadyResponse>()
    requestJsonMock
      .mockReturnValueOnce(healthRequest.promise)
      .mockReturnValueOnce(readinessRequest.promise)
    const wrapper = mountDashboard()

    await nextTick()
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('Checking')

    healthRequest.resolve(health)
    readinessRequest.resolve(readiness)
    await flushPromises()
    expect(wrapper.text()).toContain('Ready')
    expect(wrapper.text()).toContain('dashboard-service')
    expect(wrapper.text()).toContain('connected')

    requestJsonMock.mockResolvedValueOnce(health).mockResolvedValueOnce(readiness)
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(requestJsonMock).toHaveBeenCalledTimes(4)
  })

  it('renders a degraded state and the stable operational reference', async () => {
    requestJsonMock
      .mockRejectedValueOnce(
        new ApiError(503, {
          error: 'readiness_failed',
          message: 'service is not ready',
          request_id: 'dashboard-request-id',
        }),
      )
      .mockResolvedValueOnce(readiness)
    const wrapper = mountDashboard()

    await flushPromises()
    expect(wrapper.text()).toContain('Degraded')
    expect(wrapper.text()).toContain('service is not ready (Reference: dashboard-request-id)')
  })
})

function mountDashboard() {
  return mount(Dashboard, {
    global: {
      plugins: [createPinia()],
      stubs: { RouterLink: true },
    },
  })
}

function deferred<T>() {
  let resolvePromise!: (value: T) => void
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve
  })
  return { promise, resolve: resolvePromise }
}

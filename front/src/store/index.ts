import { defineStore } from 'pinia'
import { requestJson } from '@/services/http'

export interface HealthResponse {
  status: string
  service: string
}

export interface ReadyResponse extends HealthResponse {
  database: {
    kind: 'postgres' | 'sqlite'
    connected: boolean
  }
}

export const useAppStore = defineStore('app', {
  state: () => ({
    serviceName: 'cyder-template',
    health: null as HealthResponse | null,
    readiness: null as ReadyResponse | null,
    loading: false,
    error: null as string | null,
  }),

  getters: {
    isReady: (state) => state.readiness?.status === 'ready' && state.readiness.database.connected,
  },

  actions: {
    async refreshStatus() {
      this.loading = true
      this.error = null

      try {
        const [health, readiness] = await Promise.all([
          requestJson<HealthResponse>('/healthz'),
          requestJson<ReadyResponse>('/readyz'),
        ])
        this.serviceName = health.service
        this.health = health
        this.readiness = readiness
      } catch (error) {
        this.error = error instanceof Error ? error.message : 'Unable to load service status'
      } finally {
        this.loading = false
      }
    },
  },
})

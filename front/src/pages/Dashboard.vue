<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useAppStore } from '@/store'

const app = useAppStore()

const statusLabel = computed(() => {
  if (app.loading) {
    return 'Checking'
  }
  if (app.error) {
    return 'Degraded'
  }
  return app.isReady ? 'Ready' : 'Unknown'
})

const resources = [
  {
    name: 'Items',
    description: 'Example task-like records backed by the Rust API.',
    endpoint: '/api/items',
    to: '/items',
  },
  {
    name: 'Users',
    description: 'Example people records for CRUD workflows, not authentication.',
    endpoint: '/api/users',
    to: '/users',
  },
]

onMounted(() => {
  void app.refreshStatus()
})
</script>

<template>
  <main class="app-page">
    <div class="page-shell">
      <section class="page-header">
        <div>
          <p class="eyebrow">Rust + Vue template</p>
          <h1>Operator dashboard</h1>
          <p class="page-description">
            Monitor the service baseline and use the example resources as starting points for
            application workflows.
          </p>
        </div>

        <button class="secondary-button" type="button" :disabled="app.loading" @click="app.refreshStatus">
          {{ app.loading ? 'Refreshing' : 'Refresh status' }}
        </button>
      </section>

      <section class="status-panel" aria-label="Service status">
        <div class="status-summary">
          <span class="status-dot" :class="{ 'status-dot--ready': app.isReady && !app.error }" />
          <div>
            <p class="status-title">{{ statusLabel }}</p>
            <p class="status-copy">
              {{ app.error ?? 'Health and readiness checks are connected to the backend.' }}
            </p>
          </div>
        </div>

        <dl class="status-grid">
          <div>
            <dt>Service</dt>
            <dd>{{ app.health?.service ?? app.serviceName }}</dd>
          </div>
          <div>
            <dt>Health</dt>
            <dd>{{ app.health?.status ?? 'unknown' }}</dd>
          </div>
          <div>
            <dt>Database</dt>
            <dd>{{ app.readiness?.database.kind ?? 'unknown' }}</dd>
          </div>
          <div>
            <dt>Connection</dt>
            <dd>{{ app.readiness?.database.connected ? 'connected' : 'unknown' }}</dd>
          </div>
        </dl>
      </section>

      <section id="resources" class="resource-section" aria-labelledby="resources-title">
        <div class="section-heading">
          <h2 id="resources-title">Resource entry points</h2>
          <p>Open the example CRUD pages wired to the backend API.</p>
        </div>

        <div class="resource-list">
          <article v-for="resource in resources" :key="resource.name" class="resource-row">
            <div>
              <h3>{{ resource.name }}</h3>
              <p>{{ resource.description }}</p>
            </div>
            <div class="resource-actions">
              <code>{{ resource.endpoint }}</code>
              <RouterLink class="secondary-button secondary-button--compact" :to="resource.to">
                Open
              </RouterLink>
            </div>
          </article>
        </div>
      </section>
    </div>
  </main>
</template>

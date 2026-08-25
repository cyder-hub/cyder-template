<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useAppStore } from '@/store'
// template-example:start
import ExampleResources from '@/components/ExampleResources.vue'
// template-example:end

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

onMounted(() => {
  void app.refreshStatus()
})
</script>

<template>
  <main class="app-page">
    <div class="page-shell">
      <section class="page-header">
        <div>
          <p class="eyebrow">Rust + Vue application</p>
          <h1>Operator dashboard</h1>
          <p class="page-description">
            Monitor the service health, database connection, and application runtime baseline.
          </p>
        </div>

        <button
          class="secondary-button"
          type="button"
          :disabled="app.loading"
          @click="app.refreshStatus"
        >
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
      <!-- template-example:start -->
      <ExampleResources />
      <!-- template-example:end -->
    </div>
  </main>
</template>

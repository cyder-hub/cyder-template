<script setup lang="ts">
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
</script>

<template>
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
</template>

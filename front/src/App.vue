<script setup lang="ts">
const navItems = [
  { label: 'Dashboard', to: '/' },
  { label: 'Items', to: '/items' },
  { label: 'Users', to: '/users' },
]
</script>

<template>
  <div class="app-root">
    <aside class="app-sidebar" aria-label="Primary navigation">
      <RouterLink class="brand" to="/">
        <span class="brand-mark">A</span>
        <span>cyder-template</span>
      </RouterLink>

      <nav class="nav-list">
        <RouterLink
          v-for="item in navItems"
          :key="item.label"
          class="nav-link"
          :to="item.to"
        >
          {{ item.label }}
        </RouterLink>
      </nav>
    </aside>

    <div class="app-main">
      <header class="mobile-header">
        <RouterLink class="brand" to="/">
          <span class="brand-mark">A</span>
          <span>cyder-template</span>
        </RouterLink>
      </header>

      <RouterView />
    </div>
  </div>
</template>

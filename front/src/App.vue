<script setup lang="ts">
const navItems = [
  // Keep navigation entries centralized so generated projects can extend this list.
  { label: 'Dashboard', to: '/' },
  // template-example:start
  { label: 'Items', to: '/items' },
  { label: 'Users', to: '/users' },
  // template-example:end
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
        <RouterLink v-for="item in navItems" :key="item.label" class="nav-link" :to="item.to">
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

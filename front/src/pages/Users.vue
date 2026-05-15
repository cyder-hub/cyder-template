<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { createUser, deleteUser, listUsers, type User } from '@/services/users'

const users = ref<User[]>([])
const loading = ref(false)
const saving = ref(false)
const deletingId = ref<number | null>(null)
const error = ref<string | null>(null)
const name = ref('')
const email = ref('')
const active = ref(true)

const activeCount = computed(() => users.value.filter((user) => user.active).length)

onMounted(() => {
  void loadUsers()
})

async function loadUsers() {
  loading.value = true
  error.value = null

  try {
    users.value = await listUsers()
  } catch (source) {
    error.value = messageFromError(source, 'Unable to load users')
  } finally {
    loading.value = false
  }
}

async function submitUser() {
  const trimmedName = name.value.trim()
  const trimmedEmail = email.value.trim()

  if (!trimmedName) {
    error.value = 'Name is required'
    return
  }

  if (!trimmedEmail || !trimmedEmail.includes('@')) {
    error.value = 'A valid email is required'
    return
  }

  saving.value = true
  error.value = null

  try {
    const user = await createUser({
      name: trimmedName,
      email: trimmedEmail,
      active: active.value,
    })
    users.value = [user, ...users.value]
    name.value = ''
    email.value = ''
    active.value = true
  } catch (source) {
    error.value = messageFromError(source, 'Unable to create user')
  } finally {
    saving.value = false
  }
}

async function removeUser(user: User) {
  deletingId.value = user.id
  error.value = null

  try {
    await deleteUser(user.id)
    users.value = users.value.filter((current) => current.id !== user.id)
  } catch (source) {
    error.value = messageFromError(source, 'Unable to delete user')
  } finally {
    deletingId.value = null
  }
}

function formatTimestamp(value: number): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value))
}

function messageFromError(source: unknown, fallback: string): string {
  return source instanceof Error ? source.message : fallback
}
</script>

<template>
  <main class="app-page">
    <div class="page-shell">
      <section class="page-header">
        <div>
          <p class="eyebrow">Example resource</p>
          <h1>Users</h1>
          <p class="page-description">
            Manage simple people records for CRUD workflows. These records are not authentication
            identities.
          </p>
        </div>

        <button class="secondary-button" type="button" :disabled="loading" @click="loadUsers">
          {{ loading ? 'Refreshing' : 'Refresh' }}
        </button>
      </section>

      <section class="data-panel" aria-labelledby="new-user-title">
        <div class="section-heading">
          <h2 id="new-user-title">Create user</h2>
          <p>Use this as a generic CRUD example, not an access-control model.</p>
        </div>

        <form class="resource-form" @submit.prevent="submitUser">
          <label class="form-field">
            <span>Name</span>
            <input v-model="name" class="text-input" autocomplete="name" placeholder="Example User" />
          </label>

          <label class="form-field">
            <span>Email</span>
            <input
              v-model="email"
              class="text-input"
              autocomplete="email"
              inputmode="email"
              placeholder="user@example.com"
            />
          </label>

          <button
            class="toggle-button"
            type="button"
            role="switch"
            :aria-checked="active"
            :class="{ 'toggle-button--on': active }"
            @click="active = !active"
          >
            <span class="toggle-indicator" />
            <span>{{ active ? 'Active' : 'Inactive' }}</span>
          </button>

          <button class="primary-button" type="submit" :disabled="saving">
            {{ saving ? 'Creating' : 'Create user' }}
          </button>
        </form>
      </section>

      <p v-if="error" class="feedback feedback--error">{{ error }}</p>

      <section class="data-panel" aria-labelledby="users-list-title">
        <div class="section-heading section-heading--split">
          <div>
            <h2 id="users-list-title">User list</h2>
            <p>{{ users.length }} total, {{ activeCount }} active</p>
          </div>
        </div>

        <div v-if="loading" class="empty-state">Loading users</div>
        <div v-else-if="users.length === 0" class="empty-state">No users yet</div>

        <template v-else>
          <div class="table-wrapper">
            <table class="resource-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Status</th>
                  <th>Created</th>
                  <th class="action-column">Actions</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="user in users" :key="user.id">
                  <td>
                    <div class="primary-cell">{{ user.name }}</div>
                    <div class="secondary-cell">{{ user.email }}</div>
                  </td>
                  <td>
                    <span class="status-badge">{{ user.active ? 'active' : 'inactive' }}</span>
                  </td>
                  <td class="mono-cell">{{ formatTimestamp(user.created_at) }}</td>
                  <td class="action-column">
                    <button
                      class="danger-button"
                      type="button"
                      :disabled="deletingId === user.id"
                      @click="removeUser(user)"
                    >
                      {{ deletingId === user.id ? 'Deleting' : 'Delete' }}
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="mobile-record-list">
            <article v-for="user in users" :key="user.id" class="record-card">
              <div class="record-card__header">
                <h3>{{ user.name }}</h3>
                <span class="status-badge">{{ user.active ? 'active' : 'inactive' }}</span>
              </div>
              <p>{{ user.email }}</p>
              <div class="record-card__footer">
                <span>{{ formatTimestamp(user.created_at) }}</span>
                <button
                  class="danger-button"
                  type="button"
                  :disabled="deletingId === user.id"
                  @click="removeUser(user)"
                >
                  {{ deletingId === user.id ? 'Deleting' : 'Delete' }}
                </button>
              </div>
            </article>
          </div>
        </template>
      </section>
    </div>
  </main>
</template>

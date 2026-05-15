import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from '../pages/Dashboard.vue'
import Items from '../pages/Items.vue'
import Users from '../pages/Users.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'dashboard',
      component: Dashboard,
    },
    {
      path: '/items',
      name: 'items',
      component: Items,
    },
    {
      path: '/users',
      name: 'users',
      component: Users,
    },
  ],
})

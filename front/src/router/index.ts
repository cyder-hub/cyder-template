import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from '../pages/Dashboard.vue'
// template-example:start
import Items from '../pages/Items.vue'
import Users from '../pages/Users.vue'
// template-example:end

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'dashboard',
      component: Dashboard,
    },
    // template-example:start
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
    // template-example:end
  ],
})

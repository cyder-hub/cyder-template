import { fileURLToPath, URL } from 'node:url'

import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

import {
  DEV_PROXY_TARGET_ENV,
  validateProxyTarget,
} from './dev-proxy.mts'

export const createViteConfig = defineConfig(({ command, isPreview }) => {
  const isDevelopmentServer = command === 'serve' && !isPreview
  let proxyTarget: string | undefined

  if (isDevelopmentServer) {
    if (!Object.hasOwn(process.env, DEV_PROXY_TARGET_ENV)) {
      throw new Error(
        `${DEV_PROXY_TARGET_ENV} is required by the Vite development server; run \`just dev-front\` or set it explicitly`,
      )
    }
    proxyTarget = validateProxyTarget(process.env[DEV_PROXY_TARGET_ENV] ?? '')
  }

  const proxy = proxyTarget
    ? Object.fromEntries(
        ['/api', '/healthz', '/readyz'].map((path) => [
          path,
          {
            target: proxyTarget,
            changeOrigin: true,
          },
        ]),
      )
    : undefined

  return {
    envDir: false,
    plugins: [vue()],
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    server: {
      proxy,
    },
  }
})

export default createViteConfig

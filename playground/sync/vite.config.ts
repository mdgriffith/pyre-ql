import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/db': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
      '/sync': {
        target: 'ws://localhost:3000',
        ws: true,
      },
    },
  },
})

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  base: '/turnier/',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react/jsx-runtime', 'react-dom', 'react-dom/client'],
          query: ['@tanstack/react-query'],
        },
      },
    },
  },
  server: {
    host: 'localhost',
    port: 5173,
    strictPort: true,
    allowedHosts: ['localhost', '.localhost'],
    proxy: {
      '/turnier/api': {
        target: 'http://localhost:8900',
        changeOrigin: true,
      },
      '/turnier/auth': {
        target: 'http://localhost:8900',
        changeOrigin: true,
      },
    },
  },
  preview: {
    host: 'localhost',
    strictPort: true,
    allowedHosts: ['localhost', '.localhost'],
  },
})

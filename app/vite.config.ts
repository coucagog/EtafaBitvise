import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Port distinct de celui de YawREC (5173) : les deux projets peuvent tourner ensemble en développement.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5183, strictPort: true },
  build: { target: 'es2022', sourcemap: false },
})

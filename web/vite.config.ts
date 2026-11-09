import { defineConfig, type ProxyOptions } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// The dev/preview server proxies the real backend so the SPA talks to the
// same origin it is served from (cookie sessions + CSRF need that).
// `LKB_API_TARGET` overrides the backend origin.
const target = process.env.LKB_API_TARGET ?? 'http://127.0.0.1:7331'

const proxy: Record<string, string | ProxyOptions> = {
  '/api': { target, changeOrigin: false },
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    target: 'chrome120',
    reportCompressedSize: true,
    // Deterministic, inspectable output names (no hashed asset names buried in maps).
    sourcemap: false,
  },
  // Loopback IPv4 keeps the proxy reachable from headless Chrome at
  // 127.0.0.1 (bare `localhost` may resolve to ::1 on Windows).
  server: { host: '127.0.0.1', port: 5173, strictPort: true, proxy },
  preview: { host: '127.0.0.1', port: 4173, strictPort: true, proxy },
})

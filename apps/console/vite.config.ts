import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    // Same-origin in development, so the console needs no CORS entry and no
    // absolute API url. Matches how the demo serves it (nginx proxying /api),
    // which means one fewer thing that works locally and breaks deployed.
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  build: { outDir: 'dist', sourcemap: true },
});

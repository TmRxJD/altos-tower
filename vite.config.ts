import { defineConfig } from 'vite';
export default defineConfig({
  base: './',
  server: { strictPort: true, watch: { ignored: ['**/engine/target/**', '**/test-results/**'] } },
  build: { target: 'es2022' },
});

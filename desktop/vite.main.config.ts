import { defineConfig } from 'vite';

// https://vitejs.dev/config
export default defineConfig({
  build: {
    rollupOptions: {
      // node-pty loads its platform-specific .node binary with a computed
      // require path. Keep the package external so Node can resolve that
      // native module from node_modules at runtime.
      external: ['node-pty'],
    },
  },
});

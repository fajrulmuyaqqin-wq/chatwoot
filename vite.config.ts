import { defineConfig } from 'vite';
import ruby from 'vite-plugin-ruby';
import vue from '@vitejs/plugin-vue';
import { aliases, vueOptions } from './vite.shared';
import yaml from '@rollup/plugin-yaml';

export default defineConfig({
  plugins: [ruby(), vue(vueOptions), yaml()],
  css: {
    preprocessorOptions: {
      scss: {
        api: 'modern-compiler',
      },
    },
  },
  legacy: {
    skipWebSocketTokenCheck: true,
  },
  server: {
    cors: true,
    allowedHosts: true,
    hmr: {
      host: 'localhost',
      port: 3036,
      clientPort: 3036,
    },
  },
  resolve: { alias: aliases },
});

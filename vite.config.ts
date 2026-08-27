import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import svelteConfig from './svelte.config.js';

export default defineConfig({
  root: 'src/web',
  plugins: [svelte(svelteConfig)],
  build: { outDir: '../../dist/web', emptyOutDir: true }
});

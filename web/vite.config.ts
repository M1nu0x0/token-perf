import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import svelteConfig from './svelte.config.js';

export default defineConfig({
  root: 'src',
  plugins: [tailwindcss(), svelte(svelteConfig)],
  build: { outDir: '../dist', emptyOutDir: true },
  server: { proxy: { '/api': 'http://localhost:5177' } }
});

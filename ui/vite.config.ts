import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { readFileSync } from 'node:fs';

// Kubri's version, for feedback links that say which Kubri a report is about.
const { version } = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));

// In the browser the UI talks to kubri-server; in dev mode Vite forwards /api to it.
const server = process.env.KUBRI_SERVER ?? 'http://127.0.0.1:7878';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// A single-page app, shared by the Tauri desktop app and kubri-server.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	define: {
		__KUBRI_VERSION__: JSON.stringify(version)
	},
	clearScreen: false,
	server: {
		port: 5173,
		strictPort: true,
		proxy: {
			'/api': { target: server, ws: true }
		}
	}
});

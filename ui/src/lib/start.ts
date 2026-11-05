import { backend } from './api';
import { errorMessage } from './format';
import { t } from './i18n';
import { app } from './state.svelte';

/** The demo that comes with Kubri: a snapshot of a small shop, saved from a real cluster. */
export const DEMO_FILE = 'demo.kubri';

/** Where the demo is, next to the page in both apps and on a web server under a path. */
export function demoUrl(base: string): URL {
	return new URL(DEMO_FILE, base);
}

/** Reads the kubeconfig files again and starts over, as when Kubri found none it could use. */
export async function retryKubeconfig() {
	// Reading again is what finds a file made since; load() then shows its contexts, or why not.
	await backend.reloadContexts().catch(() => {});
	await app.load();
}

/** Opens the demo snapshot. Like any snapshot, it is read-only and leaves no trace. */
export async function openDemo() {
	let file: File;
	try {
		const response = await fetch(demoUrl(document.baseURI));
		if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
		file = new File([await response.blob()], DEMO_FILE);
	} catch (err) {
		app.error = t('Could not load the demo: {error}', { error: errorMessage(err) });
		return;
	}
	await app.openSnapshot(file);
}

/** Leaves a snapshot, like the demo, when the kubeconfig has no context to go to: the start page shows again. */
export async function closeSnapshot() {
	try {
		await backend.closeSnapshot();
	} catch (err) {
		app.error = errorMessage(err);
		return;
	}
	app.details = null;
	app.cluster = null;
	app.resources = [];
	await app.load();
}

import { backend, type IntegrationId } from './api';
import { errorMessage } from './format';
import { app } from './state.svelte';

/**
 * Opens a page of a tool, like a Grafana dashboard. A tool on a Service is reached through a
 * port-forward Kubri starts the first time.
 */
export async function openTool(id: IntegrationId, path: string) {
	// Browsers only open windows right after a click, not after waiting for the URL.
	const pending = backend.desktop ? null : window.open('', '_blank');
	if (pending) pending.opener = null;
	try {
		const url = (await backend.integrationUrl(id)) + path;
		if (pending) pending.location.href = url;
		else await backend.openUrl(url);
		app.refreshForwards();
	} catch (err) {
		pending?.close();
		app.toast(errorMessage(err), 'bad');
	}
}

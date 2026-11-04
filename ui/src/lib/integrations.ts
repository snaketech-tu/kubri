import { backend, type DashboardLink, type IntegrationId } from './api';
import { errorMessage } from './format';
import { app } from './state.svelte';

/** Opens a dashboard: a link from the settings as it is, one on the Grafana found through it. */
export function openDashboard(link: DashboardLink) {
	if (!link.url) return openTool('grafana', link.path);
	// Opened right away, as browsers only open windows right after a click.
	backend.openUrl(link.url).catch((err) => app.toast(errorMessage(err), 'bad'));
}

/** Copies a dashboard's whole address, to send it to someone or open it elsewhere. */
export async function copyDashboard(link: DashboardLink) {
	try {
		// A path on a Grafana in the cluster needs its address, which may start a port-forward.
		const url = link.url ?? (await backend.integrationUrl('grafana')) + link.path;
		await navigator.clipboard.writeText(url);
		app.toast(`Copied ${url}`);
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/** The host a link goes to, to tell links to several Grafanas apart. */
export function linkHost(link: DashboardLink): string {
	if (!link.url) return '';
	try {
		return new URL(link.url).host;
	} catch {
		return '';
	}
}

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

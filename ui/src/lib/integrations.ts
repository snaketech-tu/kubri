import { backend, type DashboardLink, type IntegrationId, type ObjectRef } from './api';
import { errorMessage } from './format';
import { app } from './state.svelte';

/** Opens a dashboard on the Grafana Kubri reaches. */
export function openDashboard(link: DashboardLink) {
	return openTool('grafana', link.path);
}

/** Opens the logs of an object in Grafana's Explore: a pod's, a workload's, a Service's, or its namespace's. */
export function openGrafanaLogs(target: ObjectRef) {
	return openTool(
		'grafana',
		backend.grafanaLogs(target).then((link) => link.path)
	);
}

/** Copies a dashboard's whole address, to send it to someone or open it elsewhere. */
export async function copyDashboard(link: DashboardLink) {
	try {
		// A Grafana in the cluster needs its address, which may start a port-forward.
		const url = (await backend.integrationUrl('grafana')) + link.path;
		await navigator.clipboard.writeText(url);
		app.toast(`Copied ${url}`);
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/**
 * Opens a page of a tool, like a Grafana dashboard. A tool on a Service is reached through a
 * port-forward Kubri starts the first time.
 */
export async function openTool(id: IntegrationId, path: string | Promise<string>) {
	// Browsers only open windows right after a click, not after waiting for the URL.
	const pending = backend.desktop ? null : window.open('', '_blank');
	if (pending) pending.opener = null;
	try {
		const where = await path;
		const url = (await backend.integrationUrl(id)) + where;
		if (pending) pending.location.href = url;
		else await backend.openUrl(url);
		app.refreshForwards();
	} catch (err) {
		pending?.close();
		app.toast(errorMessage(err), 'bad');
	}
}

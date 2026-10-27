import type { Backend, ResourceRef, WatchEvent } from './types';

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
	const response = await fetch(`/api${path}`, {
		method,
		headers: body === undefined ? undefined : { 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!response.ok) {
		const payload = await response.json().catch(() => null);
		throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
	}
	return response.json();
}

/** Talks to kubri-server over HTTP and WebSockets. */
export const httpBackend: Backend = {
	contexts: () => request('GET', '/contexts'),
	reloadContexts: () => request('POST', '/contexts/reload'),
	switchContext: (name) => request('PUT', '/context', { name }),
	resources: () => request('GET', '/resources'),

	watch(resource: ResourceRef, namespace, onEvent) {
		const params = new URLSearchParams({ ...resource, namespace: namespace ?? '' });
		const url = new URL(`/api/watch?${params}`, location.href);
		url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';

		let stopped = false;
		const socket = new WebSocket(url);
		socket.onmessage = (msg) => onEvent(JSON.parse(msg.data) as WatchEvent);
		socket.onclose = (e) => {
			if (!stopped && e.code !== 1000) {
				onEvent({ type: 'error', message: 'Lost connection to kubri-server' });
			}
		};
		return () => {
			stopped = true;
			socket.close();
		};
	}
};

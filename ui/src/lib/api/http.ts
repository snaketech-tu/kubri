import type { Backend, ObjectRef } from './types';

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

function objectQuery(target: ObjectRef) {
	return new URLSearchParams({
		...target.resource,
		namespace: target.namespace ?? '',
		name: target.name
	});
}

/** Talks to kubri-server over HTTP and WebSockets. */
export const httpBackend: Backend = {
	contexts: () => request('GET', '/contexts'),
	reloadContexts: () => request('POST', '/contexts/reload'),
	switchContext: (name) => request('PUT', '/context', { name }),
	resources: () => request('GET', '/resources'),

	watchTable(resource, namespace, onEvent) {
		const params = new URLSearchParams({ ...resource, namespace: namespace ?? '' });
		const url = new URL(`/api/table?${params}`, location.href);
		url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';

		let stopped = false;
		const socket = new WebSocket(url);
		socket.onmessage = (msg) => onEvent(JSON.parse(msg.data));
		socket.onclose = (e) => {
			if (!stopped && e.code !== 1000) {
				onEvent({ type: 'error', message: 'Lost connection to kubri-server' });
			}
		};
		return () => {
			stopped = true;
			socket.close();
		};
	},

	getObject: (target) => request('GET', `/object?${objectQuery(target)}`),
	replaceObject: (target, value) => request('PUT', '/object', { target, value }),
	patchObject: (target, patch) => request('PATCH', '/object', { target, patch }),
	deleteObject: (target, options) => request('DELETE', '/object', { target, options: options ?? {} }),
	scale: (target, replicas) => request('POST', '/object/scale', { target, replicas }),
	restart: (target) => request('POST', '/object/restart', { target }),
	events: (namespace, uid) =>
		request('GET', `/events?${new URLSearchParams({ namespace: namespace ?? '', uid })}`),
	metrics: (kind, namespace) =>
		request('GET', `/metrics?${new URLSearchParams({ kind, namespace: namespace ?? '' })}`)
};

import type { Backend, ObjectRef, TerminalEvent } from './types';

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

function socketUrl(path: string) {
	const url = new URL(`/api${path}`, location.href);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	return url;
}

/** Opens a socket that starts with a JSON request, like the logs and terminal endpoints. */
function requestSocket(path: string, request: unknown) {
	const socket = new WebSocket(socketUrl(path));
	socket.binaryType = 'arraybuffer';
	socket.onopen = () => socket.send(JSON.stringify(request));
	return socket;
}

/** Talks to kubri-server over HTTP and WebSockets. */
export const httpBackend: Backend = {
	contexts: () => request('GET', '/contexts'),
	reloadContexts: () => request('POST', '/contexts/reload'),
	switchContext: (name) => request('PUT', '/context', { name }),
	resources: () => request('GET', '/resources'),

	watchTable(resource, namespace, onEvent) {
		const params = new URLSearchParams({ ...resource, namespace: namespace ?? '' });
		let stopped = false;
		const socket = new WebSocket(socketUrl(`/table?${params}`));
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
		request('GET', `/metrics?${new URLSearchParams({ kind, namespace: namespace ?? '' })}`),

	streamLogs(target, options, onEvent) {
		let stopped = false;
		let ended = false;
		const socket = requestSocket('/logs', { target, options });
		socket.onmessage = (msg) => {
			const event = JSON.parse(msg.data);
			if (event.type === 'end') ended = true;
			onEvent(event);
		};
		socket.onclose = () => {
			if (stopped || ended) return;
			onEvent({ type: 'error', source: null, message: 'Lost connection to kubri-server' });
			onEvent({ type: 'end' });
		};
		return () => {
			stopped = true;
			socket.close();
		};
	},

	openTerminal(target, options, onEvent) {
		const encoder = new TextEncoder();
		let exited = false;
		let closed = false;
		const pending: string[] = [];
		const socket = requestSocket('/terminal', { target, options });
		socket.onmessage = (msg) => {
			if (msg.data instanceof ArrayBuffer) {
				onEvent({ type: 'data', data: new Uint8Array(msg.data) });
				return;
			}
			const event = JSON.parse(msg.data) as TerminalEvent;
			if (event.type === 'ready') {
				for (const data of pending.splice(0)) socket.send(encoder.encode(data));
			}
			if (event.type === 'exit') exited = true;
			onEvent(event);
		};
		socket.onclose = () => {
			if (!exited && !closed) onEvent({ type: 'exit', message: 'Lost connection to kubri-server' });
		};
		const open = () => socket.readyState === WebSocket.OPEN;
		return {
			write(data) {
				if (open() && pending.length === 0) socket.send(encoder.encode(data));
				else pending.push(data);
			},
			resize(cols, rows) {
				if (open()) socket.send(JSON.stringify({ type: 'resize', cols, rows }));
			},
			close() {
				closed = true;
				socket.close();
			}
		};
	},

	portForwards: () => request('GET', '/portforwards'),
	startPortForward: (req) => request('POST', '/portforwards', req),
	stopPortForward: (id) => request('DELETE', `/portforwards/${id}`),
	benchmarks: () => request('GET', '/benchmarks'),
	startBenchmark: (req) => request('POST', '/benchmarks', req),
	stopBenchmark: (id) => request('POST', `/benchmarks/${id}/stop`),
	removeBenchmark: (id) => request('DELETE', `/benchmarks/${id}`),
	cordon: (name, cordoned) => request('POST', '/node/cordon', { name, cordoned }),
	drain: (name, options) => request('POST', '/node/drain', { name, options }),
	pulse: (namespace) => request('GET', `/pulse?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	xray: (resource, namespace) =>
		request('GET', `/xray?${new URLSearchParams({ ...resource, namespace: namespace ?? '' })}`),
	sanitize: (namespace) => request('GET', `/sanitize?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	rbac: (namespace) => request('GET', `/rbac?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	permissions: (namespace) => request('GET', `/permissions?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	helmReleases: (namespace) => request('GET', `/helm/releases?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	helmRelease(namespace, name, revision) {
		const params = new URLSearchParams({ namespace, name });
		if (revision !== undefined) params.set('revision', String(revision));
		return request('GET', `/helm/release?${params}`);
	},
	helmHistory: (namespace, name) => request('GET', `/helm/history?${new URLSearchParams({ namespace, name })}`),
	helmVersion: () => request<{ version: string | null }>('GET', '/helm/version').then((r) => r.version),
	helmRollback: (namespace, name, revision) => request('POST', '/helm/rollback', { namespace, name, revision }),
	helmUninstall: (namespace, name) => request('POST', '/helm/uninstall', { namespace, name })
};

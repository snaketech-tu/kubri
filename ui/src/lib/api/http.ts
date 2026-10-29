import { session } from '../session.svelte';
import type { Backend, ObjectRef, TerminalEvent, TerminalSession } from './types';

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
	const response = await fetch(`/api${path}`, {
		method,
		headers: body === undefined ? undefined : { 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (response.status === 401) session.expired();
	if (!response.ok) {
		const payload = await response.json().catch(() => null);
		throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
	}
	return response.json();
}

function objectQuery(target: ObjectRef) {
	return new URLSearchParams({
		...target.resource,
		...(target.context ? { context: target.context } : {}),
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

/** A terminal over a socket: raw bytes both ways, control messages as JSON. */
function terminalSocket(path: string, request: unknown, onEvent: (event: TerminalEvent) => void): TerminalSession {
	const encoder = new TextEncoder();
	let exited = false;
	let closed = false;
	const pending: string[] = [];
	const socket = requestSocket(path, request);
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
}

/** Talks to kubri-server over HTTP and WebSockets. */
export const httpBackend: Backend = {
	contexts: () => request('GET', '/contexts'),
	reloadContexts: () => request('POST', '/contexts/reload'),
	inspectKubeconfig: (text) => request('POST', '/contexts/inspect', { text }),
	addContexts: (text, names) => request('POST', '/contexts/added', { text, names }),
	removeContext: (name) => request('DELETE', `/contexts/added?${new URLSearchParams({ name })}`),
	switchContext: (name) => request('PUT', '/context', { name }),
	resources: () => request('GET', '/resources'),
	preview: (target, change) => request('POST', '/object/preview', { target, change }),
	schema: (resource) =>
		request('GET', `/schema?${new URLSearchParams({ group: resource.group, version: resource.version, plural: resource.plural })}`),
	access: (checks, fresh = false) => request('POST', '/access', { checks, fresh }),
	whoAmI: () => request('GET', '/whoami'),
	readableNamespaces: () => request('GET', '/namespaces/readable'),
	settings: () => request('GET', '/settings'),
	setContextSettings: (name, settings) => request('PUT', '/settings/context', { name, settings }),
	setDiffSettings: (settings) => request('PUT', '/settings/diff', settings),
	setColumnSettings: (kind, columns) => request('PUT', '/settings/columns', { kind, columns }),
	setAgentSettings: (settings) => request('PUT', '/settings/agents', settings),
	agentConnection: () => request('GET', '/agents'),
	newAgentToken: () => request('POST', '/agents/token'),
	agentRequests: () => request('GET', '/agents/pending'),
	answerAgent: (id, allow) => request('POST', '/agents/answer', { id, allow }),
	agentCalls: () => request('GET', '/agents/calls'),
	setExplainSettings: (settings) => request('PUT', '/settings/explain', settings),
	explainKey: () => request('GET', '/explain/key'),
	setExplainKey: (key) => request('PUT', '/explain/key', { key }),
	explainPrompt: (subject) => request('POST', '/explain/prompt', subject),
	explain: (text) => request('POST', '/explain', { text }),
	guard: () => request('GET', '/guard'),
	unlock: () => request('POST', '/guard/unlock'),
	lock: () => request('POST', '/guard/lock'),

	watchTable(resource, namespace, scope, onEvent) {
		const params = new URLSearchParams({ ...resource, namespace: namespace ?? '' });
		if (scope?.labelSelector) params.set('labelSelector', scope.labelSelector);
		if (scope?.fieldSelector) params.set('fieldSelector', scope.fieldSelector);
		if (scope?.paths) params.set('paths', scope.paths);
		let stopped = false;
		const socket = new WebSocket(socketUrl(`/table?${params}`));
		socket.onmessage = (msg) => onEvent(JSON.parse(msg.data));
		socket.onclose = (e) => {
			if (!stopped) {
				onEvent({ type: 'error', message: 'Lost connection to kubri-server' });
			}
		};
		return () => {
			stopped = true;
			socket.close();
		};
	},

	getObject: (target) => request('GET', `/object?${objectQuery(target)}`),
	getObjectElsewhere: (target) => request('GET', `/object/elsewhere?${objectQuery(target)}`),
	replaceObject: (target, value) => request('PUT', '/object', { target, value }),
	patchObject: (target, patch) => request('PATCH', '/object', { target, patch }),
	deleteObject: (target, options) => request('DELETE', '/object', { target, options: options ?? {} }),
	scale: (target, replicas) => request('POST', '/object/scale', { target, replicas }),
	restart: (target) => request('POST', '/object/restart', { target }),
	debug: (req) => request('POST', '/debug', req),
	memoryFix: (target, container) => request('POST', '/fix/memory', { target, container }),
	setMemoryLimit: (target, container, memory) => request('POST', '/fix/memory/apply', { target, container, memory }),
	scheduling: (target) => request('POST', '/fix/scheduling', { target }),
	serviceCheck: (target) => request('POST', '/fix/service', { target }),
	apply: (body) => request('POST', '/apply', body),
	fetchManifest: (url) => request('POST', '/apply/fetch', { url }),
	drill: (target) => request('POST', '/object/drill', { target }),
	usedBy: (target) => request('POST', '/object/used-by', { target }),
	triggerCronJob: async (target) => (await request<{ name: string }>('POST', '/cronjob/trigger', { target })).name,
	suspendCronJob: (target, suspend) => request('POST', '/cronjob/suspend', { target, suspend }),
	gitops: (target, action) => request('POST', '/gitops', { target, action }),
	renewCertificate: (target) => request('POST', '/certificate/renew', { target }),
	search: (body) => request('POST', '/search', body),
	setSavedSearches: (searches) => request('PUT', '/settings/searches', searches),
	fleetCluster: (context) => request('GET', `/fleet?${new URLSearchParams({ context })}`),
	rightSizing: (namespace) => request('GET', `/right-sizing${namespace ? `?namespace=${encodeURIComponent(namespace)}` : ''}`),
	setResources: (target, container, requests, limits) => request('POST', '/object/resources', { target, container, requests, limits }),
	managedBy: (target) => request('POST', '/object/managed-by', { target }),
	setImages: (target, images) => request('POST', '/object/images', { target, images }),
	rollback: async (target) => (await request<{ name: string }>('POST', '/replicaset/rollback', { target })).name,
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

	openTerminal: (target, options, onEvent) => terminalSocket('/terminal', { target, options }, onEvent),

	portForwards: () => request('GET', '/portforwards'),
	startPortForward: (req) => request('POST', '/portforwards', req),
	startFavoriteForwards: () => request('POST', '/portforwards/favorites'),
	stopPortForward: (id) => request('DELETE', `/portforwards/${id}`),
	benchmarks: () => request('GET', '/benchmarks'),
	startBenchmark: (req) => request('POST', '/benchmarks', req),
	stopBenchmark: (id) => request('POST', `/benchmarks/${id}/stop`),
	removeBenchmark: (id) => request('DELETE', `/benchmarks/${id}`),
	cordon: (name, cordoned) => request('POST', '/node/cordon', { name, cordoned }),
	drain: (name, options) => request('POST', '/node/drain', { name, options }),
	pulse: (namespace) => request('GET', `/pulse?${new URLSearchParams({ namespace: namespace ?? '' })}`),
	integrations: (fresh) => request('GET', `/integrations?fresh=${fresh}`),
	charts: (target, range) => request('POST', '/charts', { target, range }),
	dashboards: (target) => request('POST', '/grafana/dashboards', { target }),
	integrationUrl: async (id) => ((await request('POST', '/integrations/url', { id })) as { url: string }).url,
	lokiLogs: (request_) => request('POST', '/loki/logs', request_),
	usageHistory: (namespace, seconds) =>
		request('GET', `/usage-history?${new URLSearchParams({ namespace: namespace ?? '', seconds: String(seconds) })}`),
	graph: (namespace) => request('GET', `/graph?${new URLSearchParams({ namespace: namespace ?? '' })}`),
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
	helmUninstall: (namespace, name) => request('POST', '/helm/uninstall', { namespace, name }),
	helmChange: (change) => request('POST', '/helm/change', change),
	helmChartValues: (chart) => request('POST', '/helm/chart-values', chart),
	helmSearch: (query) => request('GET', `/helm/search?${new URLSearchParams({ query })}`),
	k9sConfig: () => request('GET', '/k9s'),
	k9sSkin: (name) => request('GET', `/k9s/skins/${encodeURIComponent(name)}`),
	runPlugin: (run) => request('POST', '/plugins/run', run),
	openPlugin: (run, onEvent) => terminalSocket('/plugins/terminal', run, onEvent),
	desktop: false,
	async openUrl(url) {
		window.open(url, '_blank', 'noreferrer');
	},
	async saveText(name, text) {
		const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
		const a = document.createElement('a');
		a.href = url;
		a.download = name;
		a.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
		return null;
	}
};

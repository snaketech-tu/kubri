import { session } from '../session.svelte';
import type { Request } from './generated/Request';
import { callBackend, caller } from './rpc';
import type { Backend, TerminalEvent, TerminalSession } from './types';

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

function socketUrl(path: string) {
	const url = new URL(`/api${path}`, location.href);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	return url;
}

/** Opens a socket that starts with a JSON request, like the logs and terminal endpoints. */
function requestSocket(path: string, request: unknown) {
	const socket = new WebSocket(socketUrl(path));
	socket.binaryType = 'arraybuffer';
	socket.onopen = () => {
		// A queued open event may arrive after its owner has closed the socket.
		if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(request));
	};
	return socket;
}

/** A terminal over a socket: raw bytes both ways, control messages as JSON. */
function terminalSocket(path: string, request: unknown, onEvent: (event: TerminalEvent) => void): TerminalSession {
	const encoder = new TextEncoder();
	let state: 'connecting' | 'ready' | 'exited' | 'closed' = 'connecting';
	const pending: string[] = [];
	const socket = requestSocket(path, request);
	socket.onmessage = (msg) => {
		if (state === 'closed' || state === 'exited') return;
		if (msg.data instanceof ArrayBuffer) {
			onEvent({ type: 'data', data: new Uint8Array(msg.data) });
			return;
		}
		const event = JSON.parse(msg.data) as TerminalEvent;
		if (event.type === 'ready') {
			state = 'ready';
			for (const data of pending.splice(0)) socket.send(encoder.encode(data));
		}
		if (event.type === 'exit') {
			state = 'exited';
			pending.length = 0;
		}
		onEvent(event);
	};
	socket.onclose = () => {
		if (state === 'exited' || state === 'closed') return;
		state = 'exited';
		pending.length = 0;
		onEvent({ type: 'exit', message: 'Lost connection to kubri-server' });
	};
	const open = () => socket.readyState === WebSocket.OPEN;
	return {
		write(data) {
			if (state === 'closed' || state === 'exited') return;
			if (state === 'ready' && open()) socket.send(encoder.encode(data));
			else pending.push(data);
		},
		resize(cols, rows) {
			if (state !== 'closed' && state !== 'exited' && open()) socket.send(JSON.stringify({ type: 'resize', cols, rows }));
		},
		close() {
			if (state === 'closed') return;
			state = 'closed';
			pending.length = 0;
			socket.close();
		}
	};
}

/** A call of the core's call table. */
const send = (call: Request) => request('POST', '/call', call);

/** Talks to kubri-server over HTTP and WebSockets. */
export const httpBackend: Backend = {
	...callBackend(send),
	chooseKubeconfigFiles: async () => null,
	addKubeconfigFiles: () => Promise.reject(new Error('In the browser, add contexts from a file instead')),
	removeKubeconfigFile: () => Promise.reject(new Error('Only the desktop app chooses kubeconfig files')),
	async takeSnapshot(namespace) {
		const query = namespace ? `?${new URLSearchParams({ namespace })}` : '';
		const response = await fetch(`/api/snapshot${query}`, { method: 'POST' });
		if (response.status === 401) session.expired();
		if (!response.ok) {
			const payload = await response.json().catch(() => null);
			throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
		}
		const name = /filename="([^"]+)"/.exec(response.headers.get('content-disposition') ?? '')?.[1] ?? 'snapshot.kubri';
		const url = URL.createObjectURL(await response.blob());
		const a = document.createElement('a');
		a.href = url;
		a.download = name;
		a.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
		return null;
	},
	async openSnapshot(file) {
		const response = await fetch(`/api/snapshot?${new URLSearchParams({ name: file.name })}`, {
			method: 'PUT',
			headers: { 'content-type': 'application/octet-stream' },
			body: file
		});
		if (response.status === 401) session.expired();
		if (!response.ok) {
			const payload = await response.json().catch(() => null);
			throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
		}
		return response.json();
	},
	agentConnection: () => request('GET', '/agents'),
	newAgentToken: () => request('POST', '/agents/token'),
	agentRequests: () => request('GET', '/agents/pending'),
	answerAgent: (id, allow) => request('POST', '/agents/answer', { id, allow }),
	agentCalls: () => request('GET', '/agents/calls'),
	explainKey: () => request('GET', '/explain/key'),
	setExplainKey: (key) => request('PUT', '/explain/key', { key }),
	explainPrompt: (subject) => request('POST', '/explain/prompt', subject),
	explain: (text) => request('POST', '/explain', { text }),

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

	checkUpdate: () => caller(send)('latestRelease'),
	installUpdate: () => Promise.reject(new Error('Kubri in the browser is updated where it runs')),
	openPlugin: (run, onEvent) => terminalSocket('/plugins/terminal', run, onEvent),
	createLocalCluster: (req, onEvent) => terminalSocket('/local-cluster/create', req, onEvent),
	openLocalTerminal: (options, onEvent) => terminalSocket('/terminal/local/open', options, onEvent),
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

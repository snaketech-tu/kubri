/**
 * The live demo: the web app without kubri-server. Requests to `/api` are answered from a
 * recording of kubri-server showing the demo snapshot, made by `scripts/demo/record.cjs`. A
 * build with `VITE_KUBRI_DEMO` set to the recording's address installs this; other builds
 * leave it out.
 */

/** One request to kubri-server and its answer, as `scripts/demo/record.cjs` saw it. */
export interface RecordedRequest {
	method: string;
	/** The address, absolute or from the page's origin. */
	url: string;
	body: string | null;
	status: number;
	type: string;
	response: string;
}

/** One socket: the request it started with, if any, and what kubri-server sent on it. */
export interface RecordedSocket {
	url: string;
	first: string | null;
	/** Text, or bytes in base64. */
	messages: ({ text: string } | { binary: string })[];
	/** The server closed it after the last message. */
	closed: boolean;
}

export interface Recording {
	version: 1;
	http: RecordedRequest[];
	sockets: RecordedSocket[];
}

/** An answer to a request. */
export interface Answer {
	status: number;
	type: string;
	body: string;
}

/** A recording by key, as the demo looks things up. */
export interface Replay {
	http: Map<string, Answer>;
	/** The last answer to each call of the call table, whatever its arguments. */
	calls: Map<string, Answer>;
	sockets: Map<string, Pick<RecordedSocket, 'messages' | 'closed'>>;
}

/** What the demo answers to what it has no recording of. */
export const NOT_RECORDED = 'this is not part of the live demo, install Kubri to try it on your own cluster';

/** JSON with object keys sorted, so that equal requests give equal keys. */
export function canonical(value: unknown): string {
	if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
	if (value && typeof value === 'object') {
		const entries = Object.entries(value as Record<string, unknown>)
			.filter(([, v]) => v !== undefined)
			.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
		return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`;
	}
	return JSON.stringify(value) ?? 'null';
}

/** A body as its canonical JSON, or as it is when it is not JSON. */
function bodyKey(body: string | null | undefined): string {
	if (!body) return '';
	try {
		return ` ${canonical(JSON.parse(body))}`;
	} catch {
		return ` ${body}`;
	}
}

/** The path below `/api` with its query sorted, or null for addresses outside the API. */
export function apiPath(url: URL): string | null {
	if (url.pathname !== '/api' && !url.pathname.startsWith('/api/')) return null;
	const params = [...url.searchParams].sort(([a, x], [b, y]) => (a === b ? (x < y ? -1 : 1) : a < b ? -1 : 1));
	const query = new URLSearchParams(params).toString();
	return url.pathname.slice('/api'.length) + (query ? `?${query}` : '');
}

/** The key of an HTTP request in a recording. */
export function httpKey(method: string, path: string, body?: string | null): string {
	return `${method.toUpperCase()} ${path}${bodyKey(body)}`;
}

/** The key of a socket: its path, and the request it starts with, if it starts with one. */
export function socketKey(path: string, first?: string | null): string {
	return `WS ${path}${bodyKey(first)}`;
}

/** The name of a call to `/call`, from its body. */
function callName(path: string, body: string | null | undefined): string | null {
	if (path !== '/call' || !body) return null;
	try {
		const name = JSON.parse(body)?.call;
		return typeof name === 'string' ? name : null;
	} catch {
		return null;
	}
}

/** Looks a recording up by key; a later answer to the same request wins. */
export function index(recording: Recording): Replay {
	const replay: Replay = { http: new Map(), calls: new Map(), sockets: new Map() };
	const origin = 'http://demo';
	for (const r of recording.http) {
		const path = apiPath(new URL(r.url, origin));
		if (path === null) continue;
		const recorded = { status: r.status, type: r.type, body: r.response };
		replay.http.set(httpKey(r.method, path, r.body), recorded);
		const name = callName(path, r.body);
		if (name) replay.calls.set(name, recorded);
	}
	for (const s of recording.sockets) {
		const path = apiPath(new URL(s.url.replace(/^ws/, 'http'), origin));
		if (path !== null) replay.sockets.set(socketKey(path, s.first), { messages: s.messages, closed: s.closed });
	}
	return replay;
}

/** Calls whose answer does not depend on their arguments for the demo, such as times. */
const ANY_VARIANT = ['history', 'usageHistory', 'charts', 'latestRelease', 'integrations'];

/** Calls that change settings, which change in the browser only. */
const SETTINGS = /^set\w*(Settings|Templates|Searches)$/;

/** The recorded answer to a request, or an error that says the demo has none. */
export function answer(replay: Replay, method: string, path: string, body?: string | null): Answer {
	const found = replay.http.get(httpKey(method, path, body));
	if (found) return found;
	const name = callName(path, body);
	if (name && ANY_VARIANT.includes(name)) {
		const any = replay.calls.get(name);
		if (any) return any;
	}
	// The demo shows the settings as they were.
	if (name && SETTINGS.test(name)) {
		const reply = replay.calls.get('settings');
		if (reply?.status === 200) return { ...reply, body: JSON.stringify(JSON.parse(reply.body).settings) };
	}
	return { status: 501, type: 'application/json', body: JSON.stringify({ error: NOT_RECORDED }) };
}

/** What a socket the demo has no recording of says, in the words of its kind of socket. */
export function missingSocket(path: string): Pick<RecordedSocket, 'messages' | 'closed'> {
	const bare = path.split('?')[0];
	const messages =
		bare === '/logs'
			? [{ type: 'error', source: null, message: NOT_RECORDED }, { type: 'end' }]
			: bare === '/table'
				? [{ type: 'error', message: NOT_RECORDED }]
				: [{ type: 'exit', message: NOT_RECORDED }];
	return { messages: messages.map((m) => ({ text: JSON.stringify(m) })), closed: bare !== '/table' };
}

function decode(base64: string): ArrayBuffer {
	const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
	return bytes.buffer;
}

/** A WebSocket that plays a recorded socket: what the UI sends after its first request is ignored. */
function socketClass(recording: Promise<Replay>) {
	return class ReplaySocket extends EventTarget {
		static readonly CONNECTING = 0;
		static readonly OPEN = 1;
		static readonly CLOSING = 2;
		static readonly CLOSED = 3;
		readonly CONNECTING = 0;
		readonly OPEN = 1;
		readonly CLOSING = 2;
		readonly CLOSED = 3;
		readyState = 0;
		binaryType: BinaryType = 'blob';
		readonly url: string;
		readonly protocol = '';
		readonly extensions = '';
		readonly bufferedAmount = 0;
		onopen: ((event: Event) => void) | null = null;
		onmessage: ((event: MessageEvent) => void) | null = null;
		onclose: ((event: CloseEvent) => void) | null = null;
		onerror: ((event: Event) => void) | null = null;
		#path: string;
		#started = false;

		constructor(url: string | URL) {
			super();
			this.url = String(url);
			this.#path = apiPath(new URL(this.url)) ?? '';
			recording.then((r) => {
				if (this.readyState !== 0) return;
				this.readyState = 1;
				this.#emit('open', new Event('open'));
				// A socket the server answers at once, like a list, plays now; the others wait for their
				// request, which the UI sends as it opens.
				const now = this.#started ? undefined : r.sockets.get(socketKey(this.#path));
				if (now) this.#play(now);
			});
		}

		send(data: string | ArrayBufferLike | Blob | ArrayBufferView) {
			if (this.#started || this.readyState !== 1) return;
			this.#started = true;
			const text = typeof data === 'string' ? data : null;
			recording.then((r) => this.#play(r.sockets.get(socketKey(this.#path, text)) ?? missingSocket(this.#path)));
		}

		close() {
			if (this.readyState >= 2) return;
			this.readyState = 3;
			this.#emit('close', new CloseEvent('close', { code: 1000, wasClean: true }));
		}

		#play(socket: Pick<RecordedSocket, 'messages' | 'closed'>) {
			this.#started = true;
			const messages = [...socket.messages];
			const next = () => {
				if (this.readyState !== 1) return;
				const message = messages.shift();
				if (!message) {
					if (socket.closed) this.close();
					return;
				}
				const data = 'text' in message ? message.text : this.binaryType === 'arraybuffer' ? decode(message.binary) : new Blob([decode(message.binary)]);
				this.#emit('message', new MessageEvent('message', { data }));
				setTimeout(next);
			};
			setTimeout(next);
		}

		#emit(type: 'open' | 'message' | 'close', event: Event) {
			const handler = this[`on${type}`] as ((event: Event) => void) | null;
			handler?.call(this, event);
			this.dispatchEvent(event);
		}
	};
}

/**
 * Answers the UI's requests to `/api` from the recording at `source`, relative to the page,
 * and shows a note that this is the live demo.
 */
export function installReplay(source: string) {
	const recording: Promise<Replay> = fetch(new URL(source, document.baseURI))
		.then((r) => r.json() as Promise<Recording>)
		.then(index);
	const fetchFromNetwork = window.fetch.bind(window);
	window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
		const request = input instanceof Request ? input : null;
		const url = new URL(request ? request.url : String(input), location.href);
		const path = apiPath(url);
		if (path === null) return fetchFromNetwork(input, init);
		const method = init?.method ?? request?.method ?? 'GET';
		const body = typeof init?.body === 'string' ? init.body : null;
		const recorded = answer(await recording, method, path, body);
		return new Response(recorded.body, { status: recorded.status, headers: { 'content-type': recorded.type } });
	};
	window.WebSocket = socketClass(recording) as unknown as typeof WebSocket;
	addEventListener('DOMContentLoaded', showNote);
	if (document.readyState !== 'loading') showNote();
}

let noted = false;

/** A small note in a corner: this is a recording, and where to get Kubri. */
function showNote() {
	if (noted) return;
	noted = true;
	const note = document.createElement('aside');
	note.setAttribute('aria-label', 'Live demo');
	note.innerHTML =
		'<strong>Live demo</strong> <span>A snapshot of a small shop, read-only.</span> <a href="https://github.com/snaketech-tu/kubri#-install" target="_blank" rel="noreferrer">Get Kubri</a>';
	note.style.cssText = [
		'position:fixed',
		'left:50%',
		'bottom:44px',
		'transform:translateX(-50%)',
		'z-index:40',
		'display:flex',
		'gap:8px',
		'align-items:baseline',
		'padding:6px 14px',
		'border-radius:999px',
		'border:1px solid var(--border)',
		'background:var(--surface-1)',
		'box-shadow:var(--shadow)',
		'color:var(--text)',
		'font:13px/1.4 var(--sans, system-ui)',
		'white-space:nowrap'
	].join(';');
	document.body.append(note);
}

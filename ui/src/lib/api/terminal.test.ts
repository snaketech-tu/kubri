import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('../session.svelte', () => ({ session: { expired: vi.fn() } }));
import { httpBackend } from './http';
import type { PluginRun } from './types';

class Socket {
	static OPEN = 1;
	static latest: Socket;
	readyState = 0;
	binaryType = '';
	onopen = () => {};
	onmessage = (_event: { data: string | ArrayBuffer }) => {};
	onclose = () => {};
	send = vi.fn();
	close = vi.fn(() => { this.readyState = 3; });
	constructor(_url: URL) { Socket.latest = this; }
	event(type: string) { this.onmessage({ data: JSON.stringify({ type }) }); }
}

afterEach(() => vi.unstubAllGlobals());

function open(connect = true) {
	vi.stubGlobal('WebSocket', Socket);
	vi.stubGlobal('location', { href: 'http://localhost:7878/' });
	const event = vi.fn();
	const terminal = httpBackend.openPlugin({} as PluginRun, event);
	const socket = Socket.latest;
	if (connect) {
		socket.readyState = Socket.OPEN;
		socket.onopen();
		socket.send.mockClear();
	}
	return { terminal, socket, event };
}

describe('WebSocket terminal lifecycle', () => {
	it('sends the request before buffered input, and preserves normal output and resizing', () => {
		const { terminal, socket, event } = open(false);
		terminal.write('early');
		socket.readyState = Socket.OPEN;
		socket.onopen();
		expect(socket.send.mock.calls).toEqual([['{}']]);
		socket.event('ready');
		terminal.write('live');
		terminal.resize(100, 40);
		const data = new Uint8Array([65, 66]).buffer;
		socket.onmessage({ data });
		expect(socket.send.mock.calls.slice(1, 3).map(([bytes]) => new TextDecoder().decode(bytes))).toEqual(['early', 'live']);
		expect(socket.send).toHaveBeenLastCalledWith(JSON.stringify({ type: 'resize', cols: 100, rows: 40 }));
		expect(event.mock.calls).toEqual([[{ type: 'ready' }], [{ type: 'data', data: new Uint8Array(data) }]]);
		terminal.close();
		terminal.close();
		expect(socket.close).toHaveBeenCalledTimes(1);
	});

	it('does not send a startup request from a queued open event after closure', () => {
		const { terminal, socket } = open(false);
		terminal.close();
		socket.onopen();
		expect(socket.send).not.toHaveBeenCalled();
	});

	it('preserves resizing while the server is opening the terminal', () => {
		const { terminal, socket } = open();
		terminal.resize(100, 40);
		expect(socket.send.mock.calls).toEqual([[JSON.stringify({ type: 'resize', cols: 100, rows: 40 })]]);
	});

	it('waits for protocol readiness even when the socket is open', () => {
		const { terminal, socket } = open();
		terminal.write('a');
		terminal.write('b');
		expect(socket.send).not.toHaveBeenCalled();
		socket.event('ready');
		expect(socket.send.mock.calls.map(([bytes]) => new TextDecoder().decode(bytes))).toEqual(['a', 'b']);
	});

	it.each(['exit', 'disconnect', 'close'])('makes %s final', (ending) => {
		const { terminal, socket, event } = open();
		terminal.write('pending');
		if (ending === 'exit') socket.event('exit');
		if (ending === 'disconnect') socket.onclose();
		if (ending === 'close') terminal.close();
		const count = event.mock.calls.length;
		expect(count).toBe(ending === 'close' ? 0 : 1);
		socket.event('ready');
		socket.onmessage({ data: new ArrayBuffer(1) });
		socket.onclose();
		terminal.write('late');
		terminal.resize(80, 24);
		expect(socket.send).not.toHaveBeenCalled();
		expect(event).toHaveBeenCalledTimes(count);
	});
});

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PluginRun } from './types';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
	invoke: ipc.invoke,
	Channel: class { onmessage = (_message: unknown) => {}; }
}));
import { tauriBackend } from './tauri';

beforeEach(() => {
	ipc.invoke.mockReset();
});

function open() {
	let resolve!: (id: number) => void;
	let reject!: (error: string) => void;
	ipc.invoke.mockImplementation((command) => command === 'plugin_open'
		? new Promise<number>((yes, no) => { resolve = yes; reject = no; })
		: Promise.resolve());
	const event = vi.fn();
	const terminal = tauriBackend.openPlugin({} as PluginRun, event);
	const channel = ipc.invoke.mock.calls[0][1].onEvent;
	return { terminal, event, channel, resolve, reject };
}

describe('desktop terminal lifecycle', () => {
	it('buffers until allocation, then forwards input, output and resize with the allocated ID', async () => {
		const { terminal, event, channel, resolve } = open();
		terminal.write('early');
		channel.onmessage({ type: 'ready', container: 'app' });
		expect(ipc.invoke).toHaveBeenCalledTimes(1);
		resolve(42);
		await Promise.resolve();
		expect(ipc.invoke).toHaveBeenLastCalledWith('terminal_write', { id: 42, data: 'early' });
		terminal.resize(100, 40);
		expect(ipc.invoke).toHaveBeenLastCalledWith('terminal_resize', { id: 42, cols: 100, rows: 40 });
		const data = new Uint8Array([65]).buffer;
		channel.onmessage(data);
		expect(event.mock.calls).toEqual([
			[{ type: 'ready', container: 'app' }], [{ type: 'data', data: new Uint8Array(data) }]
		]);
		terminal.close();
		terminal.close();
		expect(ipc.invoke.mock.calls.filter(([command]) => command === 'terminal_close')).toEqual([
			['terminal_close', { id: 42 }]
		]);
	});

	it.each([true, false])('handles failed cleanup with close before allocation = %s', async (before) => {
		const { terminal, resolve } = open();
		ipc.invoke.mockImplementation(() => Promise.reject(new Error('transport gone')));
		if (before) terminal.close();
		resolve(42);
		await Promise.resolve();
		if (!before) terminal.close();
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(ipc.invoke).toHaveBeenLastCalledWith('terminal_close', { id: 42 });
	});

	it('reports an opening failure once and discards buffered input', async () => {
		const { terminal, event, reject, channel } = open();
		terminal.write('early');
		reject('permission denied');
		await Promise.resolve();
		terminal.write('late');
		terminal.resize(80, 24);
		channel.onmessage({ type: 'ready' });
		expect(event.mock.calls).toEqual([[{ type: 'exit', message: 'permission denied' }]]);
		expect(ipc.invoke).toHaveBeenCalledTimes(1);
	});

	it('drops queued input and late output when a running terminal exits during a write', async () => {
		const { terminal, event, channel, resolve } = open();
		let finish!: () => void;
		ipc.invoke.mockImplementation(() => new Promise<void>((yes) => { finish = yes; }));
		resolve(42);
		await Promise.resolve();
		terminal.write('first');
		terminal.write('pending');
		channel.onmessage({ type: 'exit', message: null });
		finish();
		await new Promise((resolve) => setTimeout(resolve, 0));
		channel.onmessage(new ArrayBuffer(1));
		terminal.write('late');
		terminal.resize(80, 24);
		expect(event.mock.calls).toEqual([[{ type: 'exit', message: null }]]);
		expect(ipc.invoke.mock.calls.map(([command]) => command)).toEqual(['plugin_open', 'terminal_write']);
	});

	it('closes a late-opened session without sending buffered input or late events', async () => {
		const { terminal, event, channel, resolve } = open();
		terminal.write('pending');
		terminal.close();
		terminal.close();
		resolve(42);
		await Promise.resolve();
		channel.onmessage({ type: 'ready' });
		terminal.write('late');
		expect(event).not.toHaveBeenCalled();
		expect(ipc.invoke.mock.calls.map(([command]) => command)).toEqual(['plugin_open', 'terminal_close']);
	});

	it('does not start input when exit precedes the open result', async () => {
		const { terminal, event, channel, resolve } = open();
		terminal.write('pending');
		channel.onmessage({ type: 'exit' });
		resolve(42);
		await Promise.resolve();
		terminal.write('late');
		terminal.resize(80, 24);
		channel.onmessage({ type: 'ready' });
		expect(event).toHaveBeenCalledTimes(1);
		expect(ipc.invoke).toHaveBeenCalledTimes(1);
	});

	it('ignores opening errors after local closure', async () => {
		const { terminal, event, reject } = open();
		terminal.close();
		reject('failed');
		await Promise.resolve();
		expect(event).not.toHaveBeenCalled();
	});
});

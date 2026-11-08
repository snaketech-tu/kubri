import { describe, expect, it, vi } from 'vitest';
import { writeQueue } from './queue';

describe('terminal input lifecycle', () => {
	it('reports a failed write and continues with input queued behind it', async () => {
		let reject!: (err: Error) => void;
		const send = vi.fn((_data: string) => Promise.resolve()).mockImplementationOnce(
			() => new Promise<void>((_resolve, no) => { reject = no; })
		);
		const error = vi.fn();
		const queue = writeQueue(send, error);
		queue.start();
		queue.write('first');
		queue.write('second');
		const failure = new Error('write failed');
		reject(failure);
		await vi.waitFor(() => expect(send.mock.calls).toEqual([['first'], ['second']]));
		expect(error).toHaveBeenCalledExactlyOnceWith(failure);
	});

	it('does not report a write failure or send more input after closing', async () => {
		let reject!: (err: Error) => void;
		const send = vi.fn(() => new Promise<void>((_resolve, no) => { reject = no; }));
		const error = vi.fn();
		const queue = writeQueue(send, error);
		queue.start();
		queue.write('first');
		queue.write('second');
		queue.close();
		reject(new Error('transport disappeared'));
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(error).not.toHaveBeenCalled();
		expect(send).toHaveBeenCalledTimes(1);
	});

	it('waits for readiness and serializes buffered input', async () => {
		let finish!: () => void;
		const send = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
		const queue = writeQueue(send, vi.fn());
		queue.write('a');
		expect(send).not.toHaveBeenCalled();
		queue.start();
		queue.write('b');
		queue.write('c');
		expect(send.mock.calls).toEqual([['a']]);
		finish();
		await vi.waitFor(() => expect(send.mock.calls).toEqual([['a'], ['bc']]));
		finish();
	});

	it('cannot restart after closing before readiness', () => {
		const send = vi.fn(async () => {});
		const queue = writeQueue(send, vi.fn());
		queue.write('old');
		queue.close();
		queue.start();
		queue.write('new');
		expect(send).not.toHaveBeenCalled();
	});

	it('discards pending input when closed during a write', async () => {
		let finish!: () => void;
		const send = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
		const queue = writeQueue(send, vi.fn());
		queue.start();
		queue.write('a');
		queue.write('b');
		queue.close();
		finish();
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(send).toHaveBeenCalledTimes(1);
	});
});

/**
 * Sends terminal input in order with one request in flight at a time. Keystrokes typed
 * while a request is pending are sent together in the next one.
 */
export function writeQueue(send: (data: string) => Promise<unknown>, onError: (err: unknown) => void) {
	let pending = '';
	let busy = false;
	let state: 'waiting' | 'ready' | 'closed' = 'waiting';

	function flush() {
		if (busy || state !== 'ready' || !pending) return;
		const data = pending;
		pending = '';
		busy = true;
		send(data)
			.catch((err) => {
				if (state !== 'closed') onError(err);
			})
			.finally(() => {
				busy = false;
				flush();
			});
	}

	return {
		write(data: string) {
			if (state === 'closed') return;
			pending += data;
			flush();
		},
		/** Starts sending; input written before is kept until then. */
		start() {
			if (state === 'closed') return;
			state = 'ready';
			flush();
		},
		/** Closing is final; an in-flight write may finish but queued input is discarded. */
		close() {
			state = 'closed';
			pending = '';
		}
	};
}

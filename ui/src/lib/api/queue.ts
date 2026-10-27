/**
 * Sends terminal input in order with one request in flight at a time. Keystrokes typed
 * while a request is pending are sent together in the next one.
 */
export function writeQueue(send: (data: string) => Promise<unknown>, onError: (err: unknown) => void) {
	let pending = '';
	let busy = false;
	let ready = false;

	function flush() {
		if (busy || !ready || !pending) return;
		const data = pending;
		pending = '';
		busy = true;
		send(data)
			.catch(onError)
			.finally(() => {
				busy = false;
				flush();
			});
	}

	return {
		write(data: string) {
			pending += data;
			flush();
		},
		/** Starts sending; input written before is kept until then. */
		start() {
			ready = true;
			flush();
		}
	};
}

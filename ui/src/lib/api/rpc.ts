// The calls of the core's call table (`crates/kubri-core/src/call.rs`), written once for both
// apps: each backend only says how a request reaches the core.

import type { Request } from './generated/Request';
import type { Responses } from './generated/Responses';
import type { Backend } from './types';

export type CallName = Request['call'];
export type CallArgs<C extends CallName> = Omit<Extract<Request, { call: C }>, 'call'>;

/** Sends one request to the core and resolves with its answer. */
export type Transport = (request: Request) => Promise<unknown>;

/** Makes a call by name, with its arguments; calls without arguments take none. */
export function caller(send: Transport) {
	return <C extends CallName>(call: C, ...args: keyof CallArgs<C> extends never ? [] : [CallArgs<C>]) =>
		send({ call, ...args[0] } as Request) as Promise<Responses[C]>;
}

/** The part of `Backend` that goes through the call table. */
export type CallBackend = Pick<
	Backend,
	| 'contexts'
	| 'reloadContexts'
	| 'kubeconfigFiles'
	| 'inspectKubeconfig'
	| 'addContexts'
	| 'removeContext'
	| 'switchContext'
	| 'cancelSwitch'
>;

export function callBackend(send: Transport): CallBackend {
	const call = caller(send);
	return {
		contexts: () => call('contexts'),
		reloadContexts: () => call('reloadContexts'),
		kubeconfigFiles: () => call('kubeconfigFiles'),
		inspectKubeconfig: (text) => call('inspectKubeconfig', { text }),
		addContexts: (text, names) => call('addContexts', { text, names }),
		removeContext: (name) => call('removeContext', { name }),
		switchContext: (name) => call('switchContext', { name }),
		cancelSwitch: async () => {
			await call('cancelSwitch');
		}
	};
}

import { Channel, invoke } from '@tauri-apps/api/core';
import type { Backend, WatchEvent } from './types';

/** Talks to the Rust side of the desktop app through Tauri commands. */
export const tauriBackend: Backend = {
	contexts: () => invoke('contexts'),
	reloadContexts: () => invoke('reload_contexts'),
	switchContext: (name) => invoke('switch_context', { name }),
	resources: () => invoke('resources'),

	watch(resource, namespace, onEvent) {
		const channel = new Channel<WatchEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('watch', { resource, namespace, onEvent: channel }).catch((err) => {
			onEvent({ type: 'error', message: String(err) });
			return null;
		});
		return () => {
			id.then((id) => id !== null && invoke('unwatch', { id }));
		};
	}
};

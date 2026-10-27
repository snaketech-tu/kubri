import { Channel, invoke } from '@tauri-apps/api/core';
import type { Backend, TableEvent } from './types';

/** Talks to the Rust side of the desktop app through Tauri commands. */
export const tauriBackend: Backend = {
	contexts: () => invoke('contexts'),
	reloadContexts: () => invoke('reload_contexts'),
	switchContext: (name) => invoke('switch_context', { name }),
	resources: () => invoke('resources'),

	watchTable(resource, namespace, onEvent) {
		const channel = new Channel<TableEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('watch_table', { resource, namespace, onEvent: channel }).catch((err) => {
			onEvent({ type: 'error', message: String(err) });
			return null;
		});
		return () => {
			id.then((id) => id !== null && invoke('unwatch', { id }));
		};
	},

	getObject: (target) => invoke('get_object', { target }),
	replaceObject: (target, value) => invoke('replace_object', { target, value }),
	patchObject: (target, patch) => invoke('patch_object', { target, patch }),
	deleteObject: (target, options) => invoke('delete_object', { target, options: options ?? null }),
	scale: (target, replicas) => invoke('scale', { target, replicas }),
	restart: (target) => invoke('restart', { target }),
	events: (namespace, uid) => invoke('events', { namespace, uid }),
	metrics: (kind, namespace) => invoke('metrics', { kind, namespace })
};

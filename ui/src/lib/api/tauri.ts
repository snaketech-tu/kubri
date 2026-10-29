import { Channel, invoke } from '@tauri-apps/api/core';
import { writeQueue } from './queue';
import type { Backend, LogEvent, TableEvent, TerminalEvent, TerminalSession } from './types';

/** Opens a terminal with `command`; all terminals share the write, resize and close commands. */
function terminalChannel(
	command: string,
	args: Record<string, unknown>,
	onEvent: (event: TerminalEvent) => void
): TerminalSession {
	// Output arrives as raw bytes, control messages as JSON.
	const channel = new Channel<ArrayBuffer | Exclude<TerminalEvent, { type: 'data' }>>();
	channel.onmessage = (msg) => {
		onEvent(msg instanceof ArrayBuffer ? { type: 'data', data: new Uint8Array(msg) } : msg);
	};
	let id: number | null = null;
	let closed = false;
	const input = writeQueue(
		(data) => invoke('terminal_write', { id, data }),
		() => {}
	);
	invoke<number>(command, { ...args, onEvent: channel }).then(
		(opened) => {
			id = opened;
			if (closed) invoke('terminal_close', { id });
			else input.start();
		},
		(err) => onEvent({ type: 'exit', message: String(err) })
	);
	return {
		write: (data) => input.write(data),
		resize(cols, rows) {
			if (id !== null && !closed) invoke('terminal_resize', { id, cols, rows }).catch(() => {});
		},
		close() {
			closed = true;
			if (id !== null) invoke('terminal_close', { id });
		}
	};
}

/** Talks to the Rust side of the desktop app through Tauri commands. */
export const tauriBackend: Backend = {
	contexts: () => invoke('contexts'),
	reloadContexts: () => invoke('reload_contexts'),
	switchContext: (name) => invoke('switch_context', { name }),
	resources: () => invoke('resources'),
	preview: (target, change) => invoke('preview', { target, change }),
	schema: (resource) => invoke('schema', { resource }),
	access: (checks, fresh = false) => invoke('access', { checks, fresh }),
	whoAmI: () => invoke('who_am_i'),
	readableNamespaces: () => invoke('readable_namespaces'),
	settings: () => invoke('settings'),
	setContextSettings: (name, settings) => invoke('set_context_settings', { name, settings }),
	setDiffSettings: (settings) => invoke('set_diff_settings', { settings }),
	guard: () => invoke('guard'),
	unlock: () => invoke('unlock'),
	lock: () => invoke('lock'),

	watchTable(resource, namespace, scope, onEvent) {
		const channel = new Channel<TableEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('watch_table', { resource, namespace, scope, onEvent: channel }).catch((err) => {
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
	drill: (target) => invoke('drill', { target }),
	usedBy: (target) => invoke('used_by', { target }),
	triggerCronJob: (target) => invoke('trigger_cronjob', { target }),
	suspendCronJob: (target, suspend) => invoke('suspend_cronjob', { target, suspend }),
	setImages: (target, images) => invoke('set_images', { target, images }),
	rollback: (target) => invoke('rollback', { target }),
	events: (namespace, uid) => invoke('events', { namespace, uid }),
	metrics: (kind, namespace) => invoke('metrics', { kind, namespace }),

	streamLogs(target, options, onEvent) {
		const channel = new Channel<LogEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('logs', { target, options, onEvent: channel }).catch((err) => {
			onEvent({ type: 'error', source: null, message: String(err) });
			onEvent({ type: 'end' });
			return null;
		});
		return () => {
			id.then((id) => id !== null && invoke('unwatch', { id }));
		};
	},

	openTerminal: (target, options, onEvent) => terminalChannel('terminal_open', { target, options }, onEvent),

	portForwards: () => invoke('port_forwards'),
	startPortForward: (request) => invoke('start_port_forward', { request }),
	stopPortForward: (id) => invoke('stop_port_forward', { id }),
	benchmarks: () => invoke('benchmarks'),
	startBenchmark: (request) => invoke('start_benchmark', { request }),
	stopBenchmark: (id) => invoke('stop_benchmark', { id }),
	removeBenchmark: (id) => invoke('remove_benchmark', { id }),
	cordon: (name, cordoned) => invoke('cordon', { name, cordoned }),
	drain: (name, options) => invoke('drain', { name, options }),
	pulse: (namespace) => invoke('pulse', { namespace }),
	xray: (resource, namespace) => invoke('xray', { resource, namespace }),
	sanitize: (namespace) => invoke('sanitize', { namespace }),
	rbac: (namespace) => invoke('rbac', { namespace }),
	permissions: (namespace) => invoke('permissions', { namespace }),
	helmReleases: (namespace) => invoke('helm_releases', { namespace }),
	helmRelease: (namespace, name, revision) => invoke('helm_release', { namespace, name, revision: revision ?? null }),
	helmHistory: (namespace, name) => invoke('helm_history', { namespace, name }),
	helmVersion: () => invoke('helm_version'),
	helmRollback: (namespace, name, revision) => invoke('helm_rollback', { namespace, name, revision }),
	helmUninstall: (namespace, name) => invoke('helm_uninstall', { namespace, name }),
	k9sConfig: () => invoke('k9s_config'),
	k9sSkin: (name) => invoke('k9s_skin', { name }),
	runPlugin: (run) => invoke('plugin_run', { run }),
	openPlugin: (run, onEvent) => terminalChannel('plugin_open', { run }, onEvent),
	desktop: true,
	openUrl: (url) => invoke('open_url', { url }),
	saveText: (name, text) => invoke('save_text', { name, text })
};

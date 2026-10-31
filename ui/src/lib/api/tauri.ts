import { Channel, invoke } from '@tauri-apps/api/core';
import { writeQueue } from './queue';
import type { Backend, LogEvent, TableEvent, TerminalEvent, TerminalSession, UpdateProgress } from './types';

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
	inspectKubeconfig: (text) => invoke('inspect_kubeconfig', { text }),
	addContexts: (text, names) => invoke('add_contexts', { text, names }),
	removeContext: (name) => invoke('remove_context', { name }),
	switchContext: (name) => invoke('switch_context', { name }),
	takeSnapshot: (namespace) => invoke('take_snapshot', { namespace }),
	openSnapshot: async (file) =>
		invoke('open_snapshot', new Uint8Array(await file.arrayBuffer()), { headers: { 'x-name': encodeURIComponent(file.name) } }),
	resources: () => invoke('resources'),
	preview: (target, change) => invoke('preview', { target, change }),
	schema: (resource) => invoke('schema', { resource }),
	access: (checks, fresh = false) => invoke('access', { checks, fresh }),
	whoAmI: () => invoke('who_am_i'),
	readableNamespaces: () => invoke('readable_namespaces'),
	settings: () => invoke('settings'),
	setContextSettings: (name, settings) => invoke('set_context_settings', { name, settings }),
	setDiffSettings: (settings) => invoke('set_diff_settings', { settings }),
	setHistorySettings: (settings) => invoke('set_history_settings', { settings }),
	history: (query) => invoke('history', { query }),
	historyDetail: (id) => invoke('history_detail', { id }),
	setColumnSettings: (kind, columns) => invoke('set_column_settings', { kind, columns }),
	setAgentSettings: (settings) => invoke('set_agent_settings', { settings }),
	agentConnection: () => invoke('agents_connection'),
	newAgentToken: () => invoke('agents_new_token'),
	agentRequests: () => invoke('agents_pending'),
	answerAgent: (id, allow) => invoke('agents_answer', { id, allow }),
	agentCalls: () => invoke('agents_calls'),
	setExplainSettings: (settings) => invoke('set_explain_settings', { settings }),
	explainKey: () => invoke('explain_key'),
	setExplainKey: (key) => invoke('set_explain_key', { key }),
	explainPrompt: (subject) => invoke('explain_prompt', { subject }),
	explain: (text) => invoke('explain', { text }),
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
	getObjectElsewhere: (target) => invoke('get_object_elsewhere', { target }),
	replaceObject: (target, value) => invoke('replace_object', { target, value }),
	patchObject: (target, patch) => invoke('patch_object', { target, patch }),
	deleteObject: (target, options) => invoke('delete_object', { target, options: options ?? null }),
	scale: (target, replicas) => invoke('scale', { target, replicas }),
	restart: (target) => invoke('restart', { target }),
	debug: (request) => invoke('debug', { request }),
	memoryFix: (target, container) => invoke('memory_fix', { target, container }),
	setMemoryLimit: (target, container, memory) => invoke('set_memory_limit', { target, container, memory }),
	scheduling: (target) => invoke('scheduling', { target }),
	serviceCheck: (target) => invoke('service_check', { target }),
	apply: (request) => invoke('apply', { request }),
	fetchManifest: (url) => invoke('fetch_manifest', { url }),
	drill: (target) => invoke('drill', { target }),
	usedBy: (target) => invoke('used_by', { target }),
	triggerCronJob: (target) => invoke('trigger_cronjob', { target }),
	suspendCronJob: (target, suspend) => invoke('suspend_cronjob', { target, suspend }),
	gitops: (target, action) => invoke('gitops', { target, action }),
	renewCertificate: (target) => invoke('renew_certificate', { target }),
	search: (request) => invoke('search', { request }),
	setSavedSearches: (searches) => invoke('set_saved_searches', { searches }),
	fleetCluster: (context) => invoke('fleet_cluster', { context }),
	rightSizing: (namespace) => invoke('right_sizing', { namespace }),
	setResources: (target, container, requests, limits) => invoke('set_resources', { target, container, requests, limits }),
	managedBy: (target) => invoke('managed_by', { target }),
	views: () => invoke('views'),
	renderView: (resource, object) => invoke('render_view', { resource, object }),
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
	startFavoriteForwards: () => invoke('start_favorite_forwards'),
	stopPortForward: (id) => invoke('stop_port_forward', { id }),
	benchmarks: () => invoke('benchmarks'),
	startBenchmark: (request) => invoke('start_benchmark', { request }),
	stopBenchmark: (id) => invoke('stop_benchmark', { id }),
	removeBenchmark: (id) => invoke('remove_benchmark', { id }),
	cordon: (name, cordoned) => invoke('cordon', { name, cordoned }),
	drain: (name, options) => invoke('drain', { name, options }),
	pulse: (namespace) => invoke('pulse', { namespace }),
	integrations: (fresh) => invoke('integrations', { fresh }),
	charts: (target, range) => invoke('charts', { target, range }),
	dashboards: (target) => invoke('dashboards', { target }),
	integrationUrl: (id) => invoke('integration_url', { id }),
	lokiLogs: (request) => invoke('loki_logs', { request }),
	usageHistory: (namespace, seconds) => invoke('usage_history', { namespace, seconds }),
	xray: (resource, namespace) => invoke('xray', { resource, namespace }),
	graph: (namespace) => invoke('graph', { namespace }),
	sanitize: (namespace) => invoke('sanitize', { namespace }),
	rbac: (namespace) => invoke('rbac', { namespace }),
	permissions: (namespace) => invoke('permissions', { namespace }),
	helmReleases: (namespace) => invoke('helm_releases', { namespace }),
	helmRelease: (namespace, name, revision) => invoke('helm_release', { namespace, name, revision: revision ?? null }),
	helmHistory: (namespace, name) => invoke('helm_history', { namespace, name }),
	helmVersion: () => invoke('helm_version'),
	latestRelease: () => invoke('latest_release'),
	checkUpdate: () => invoke('update_check'),
	installUpdate(onProgress) {
		const channel = new Channel<UpdateProgress>();
		channel.onmessage = onProgress;
		return invoke('update_install', { onProgress: channel });
	},
	helmRollback: (namespace, name, revision) => invoke('helm_rollback', { namespace, name, revision }),
	helmUninstall: (namespace, name) => invoke('helm_uninstall', { namespace, name }),
	helmChange: (change) => invoke('helm_change', { change }),
	helmChartValues: (chart) => invoke('helm_chart_values', { chart }),
	helmSearch: (query) => invoke('helm_search', { query }),
	k9sConfig: () => invoke('k9s_config'),
	k9sSkin: (name) => invoke('k9s_skin', { name }),
	runPlugin: (run) => invoke('plugin_run', { run }),
	openPlugin: (run, onEvent) => terminalChannel('plugin_open', { run }, onEvent),
	desktop: true,
	openUrl: (url) => invoke('open_url', { url }),
	saveText: (name, text) => invoke('save_text', { name, text })
};

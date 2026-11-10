// The calls of the core's call table (`crates/kubri-core/src/call.rs`), written once for both
// apps: each backend only says how a request reaches the core.

import type { KindSchema } from '../schema';
import type { Request } from './generated/Request';
import type { Responses } from './generated/Responses';
import type { JsonValue } from './generated/serde_json/JsonValue';
import type { Backend, KubeObject } from './types';

export type CallName = Request['call'];
/** `Omit` for each member of a union, so that requests made of several shapes keep them. */
type OmitEach<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
export type CallArgs<C extends CallName> = OmitEach<Extract<Request, { call: C }>, 'call'>;

/** Sends one request to the core and resolves with its answer. */
export type Transport = (request: Request) => Promise<unknown>;

/** Makes a call by name, with its arguments; calls without arguments take none. */
export function caller(send: Transport) {
	return <C extends CallName>(call: C, ...args: keyof CallArgs<C> extends never ? [] : [CallArgs<C>]) =>
		send({ call, ...args[0] } as Request) as Promise<Responses[C]>;
}

/** The part of `Backend` that goes through the call table. */
export type CallBackend = Omit<
	Backend,
	| 'chooseKubeconfigFiles'
	| 'addKubeconfigFiles'
	| 'removeKubeconfigFile'
	| 'takeSnapshot'
	| 'openSnapshot'
	| 'agentConnection'
	| 'newAgentToken'
	| 'agentRequests'
	| 'answerAgent'
	| 'agentCalls'
	| 'explainKey'
	| 'setExplainKey'
	| 'explainPrompt'
	| 'explain'
	| 'watchTable'
	| 'streamLogs'
	| 'openTerminal'
	| 'openPlugin'
	| 'createLocalCluster'
	| 'openLocalTerminal'
	| 'checkUpdate'
	| 'installUpdate'
	| 'desktop'
	| 'openUrl'
	| 'saveText'
>;

/** Waits for a call that answers nothing. */
async function done(reply: Promise<unknown>): Promise<void> {
	await reply;
}

export function callBackend(send: Transport): CallBackend {
	const call = caller(send);
	const object = (reply: Promise<JsonValue>) => reply as Promise<unknown> as Promise<KubeObject>;
	return {
		contexts: () => call('contexts'),
		reloadContexts: () => call('reloadContexts'),
		kubeconfigFiles: () => call('kubeconfigFiles'),
		inspectKubeconfig: (text) => call('inspectKubeconfig', { text }),
		addContexts: (text, names) => call('addContexts', { text, names }),
		removeContext: (name) => call('removeContext', { name }),
		switchContext: (name) => call('switchContext', { name }),
		cancelSwitch: () => done(call('cancelSwitch')),
		closeSnapshot: () => done(call('closeSnapshot')),

		resources: () => call('resources'),
		refreshResources: () => call('refreshResources'),
		schema: async (resource) => (await call('schema', { resource })) as KindSchema | null,
		access: (checks, fresh = false) => call('access', { checks, fresh }),
		whoAmI: () => call('whoAmI'),
		readableNamespaces: () => call('readableNamespaces'),
		guard: () => call('guard'),
		unlock: () => call('unlock'),
		lock: () => call('lock'),

		settings: () => call('settings'),
		setContextSettings: (name, settings) => call('setContextSettings', { name, settings }),
		setDiffSettings: (settings) => call('setDiffSettings', { settings }),
		setHistorySettings: (settings) => call('setHistorySettings', { settings }),
		setConnectionSettings: (settings) => call('setConnectionSettings', { settings }),
		setTerminalSettings: (settings) => call('setTerminalSettings', { settings }),
		setLabelTemplates: (templates) => call('setLabelTemplates', { templates }),
		setColumnSettings: (kind, columns) => call('setColumnSettings', { kind, columns }),
		setAgentSettings: (settings) => call('setAgentSettings', { settings }),
		setExplainSettings: (settings) => call('setExplainSettings', { settings }),
		setSavedSearches: (searches) => call('setSavedSearches', { searches }),

		history: (query) => call('history', query),
		historyDetail: (id) => call('historyDetail', { id }),

		getObject: (target) => object(call('getObject', { target })),
		getObjectElsewhere: (target) => object(call('getObjectElsewhere', { target })),
		replaceObject: (target, value) => object(call('replaceObject', { target, value: value as JsonValue })),
		patchObject: (target, patch) => object(call('patchObject', { target, patch: patch as JsonValue })),
		deleteObject: (target, options) => done(call('deleteObject', { target, options: options ?? null })),
		preview: (target, change) => call('preview', { target, change }),
		scale: (target, replicas) => done(call('scale', { target, replicas })),
		restart: (target) => done(call('restart', { target })),
		debug: (request) => call('debug', request),
		apply: (request) => call('apply', request),
		fetchManifest: (url) => call('fetchManifest', { url }),
		drill: (target) => call('drill', { target }),
		usedBy: (target) => call('usedBy', { target }),
		managedBy: (target) => call('managedBy', { target }),
		events: (namespace, uid) => call('events', { namespace, uid }),
		triggerCronJob: (target) => call('triggerCronJob', { target }),
		suspendCronJob: (target, suspend) => done(call('suspendCronJob', { target, suspend })),
		gitops: (target, action) => done(call('gitops', { target, action })),
		renewCertificate: (target) => done(call('renewCertificate', { target })),
		setImages: (target, images) => done(call('setImages', { target, images })),
		rollback: (target) => call('rollback', { target }),

		memoryFix: (target, container) => call('memoryFix', { target, container }),
		setMemoryLimit: (target, container, memory) => done(call('setMemoryLimit', { target, container, memory })),
		scheduling: (target) => call('scheduling', { target }),
		serviceCheck: (target) => call('serviceCheck', { target }),
		capacity: (target) => call('capacity', { target }),
		rightSizing: (namespace) => call('rightSizing', { namespace }),
		setResources: (target, containers) => done(call('setResources', { target, containers })),

		search: (request) => call('search', request),
		fleetCluster: (context) => call('fleetCluster', { context }),
		views: () => call('views'),
		renderView: (resource, object) => call('renderView', { resource, object: object as JsonValue }),
		metrics: (kind, namespace) => call('metrics', { kind, namespace }),
		pulse: (namespace) => call('pulse', { namespace }),
		xray: (resource, namespace) => call('xray', { resource, namespace }),
		graph: (namespace) => call('graph', { namespace }),
		sanitize: (namespace) => call('sanitize', { namespace }),
		rbac: (namespace) => call('rbac', { namespace }),
		permissions: (namespace) => call('permissions', { namespace }),

		integrations: (fresh) => call('integrations', { fresh }),
		charts: (target, range) => call('charts', { target, range }),
		usageHistory: (namespace, seconds) => call('usageHistory', { namespace, seconds }),
		dashboards: (target) => call('dashboards', { target }),
		grafanaLogs: (target) => call('grafanaLogs', { target }),
		integrationUrl: (id) => call('integrationUrl', { id }),
		lokiLogs: (request) => call('lokiLogs', request),

		portForwards: () => call('portForwards'),
		startPortForward: (request) => call('startPortForward', request),
		startFavoriteForwards: () => call('startFavoriteForwards'),
		stopPortForward: (id) => done(call('stopPortForward', { id })),
		benchmarks: () => call('benchmarks'),
		startBenchmark: (request) => call('startBenchmark', request),
		stopBenchmark: (id) => done(call('stopBenchmark', { id })),
		removeBenchmark: (id) => done(call('removeBenchmark', { id })),
		cordon: (name, cordoned) => done(call('cordon', { name, cordoned })),
		drain: (name, options) => call('drain', { name, options }),

		helmReleases: (namespace) => call('helmReleases', { namespace }),
		helmRelease: (namespace, name, revision) => call('helmRelease', { namespace, name, revision: revision ?? null }),
		helmHistory: (namespace, name) => call('helmHistory', { namespace, name }),
		helmVersion: () => call('helmVersion'),
		helmRollback: (namespace, name, revision) => call('helmRollback', { namespace, name, revision }),
		helmUninstall: (namespace, name) => call('helmUninstall', { namespace, name }),
		helmChange: (change) => call('helmChange', change),
		helmChartValues: (chart) => call('helmChartValues', chart),
		helmSearch: (query) => call('helmSearch', { query }),
		latestRelease: () => call('latestRelease'),

		k9sConfig: () => call('k9sConfig'),
		k9sSkin: (name) => call('k9sSkin', { name }),
		runPlugin: (run) => done(call('runPlugin', run)),
		localTerminalInfo: () => call('localTerminalInfo'),
		localClusters: () => call('localClusters')
	};
}

import {
	backend,
	type ClusterInfo,
	type BenchInfo,
	type ContextInfo,
	type HelmRelease,
	type K9sConfig,
	type ObjectRef,
	type Plugin,
	type PluginRun,
	type PortForwardInfo,
	type ResourceInfo,
	type ResourceRef,
	type TerminalMode
} from './api';
import type { Action, ConfigSection, PaletteConfig } from './commands';
import { errorMessage } from './format';
import { skinTheme, type Focus, type PluginView } from './k9s';
import { applyTheme, resolveColor, storeSkin, storedSkin } from './skin';

export const ALL_NAMESPACES = '';

export type DetailsTab = 'describe' | 'yaml' | 'logs' | 'shell';

export interface Details {
	target: ObjectRef;
	resource: ResourceInfo;
	tab: DetailsTab;
	editing: boolean;
	/** Start the logs with the previous container instance. */
	previous?: boolean;
	/** How the shell tab connects. */
	mode?: TerminalMode;
}

export type Page =
	| 'resources'
	| 'portforwards'
	| 'benchmarks'
	| 'pulses'
	| 'xray'
	| 'rbac'
	| 'helm'
	| 'sanitizer'
	| 'config';

export type Dialog =
	| { kind: 'delete'; resource: ResourceInfo; targets: ObjectRef[] }
	| { kind: 'scale'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'restart'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'labels'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'portforward'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'benchmark'; forward: PortForwardInfo; preset?: BenchInfo }
	| { kind: 'drain'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'helmRollback'; release: HelmRelease; revision: number; done: () => void }
	| { kind: 'helmUninstall'; release: HelmRelease; done: () => void }
	| { kind: 'plugin'; plugin: Plugin; run: PluginRun; command: string };

export interface Toast {
	id: number;
	tone: 'ok' | 'bad';
	text: string;
}

/** Global app state: kubeconfig contexts, the connected cluster and what is being browsed. */
class AppState {
	contexts = $state<ContextInfo[]>([]);
	cluster = $state<ClusterInfo | null>(null);
	resources = $state<ResourceInfo[]>([]);
	namespaces = $state<string[]>([]);
	selected = $state<ResourceInfo | null>(null);
	namespace = $state(ALL_NAMESPACES);
	connecting = $state<string | null>(null);
	error = $state<string | null>(null);

	page = $state<Page>('resources');
	/** The kind the XRay page starts its trees from. */
	xrayResource = $state<ResourceInfo | null>(null);
	details = $state<Details | null>(null);
	/** The details panel fills the window, for logs and shells. */
	maximized = $state(false);
	dialog = $state<Dialog | null>(null);
	forwards = $state<PortForwardInfo[]>([]);
	benchmarks = $state<BenchInfo[]>([]);
	/** The benchmark the benchmarks page shows. */
	benchmarkId = $state<number | null>(null);
	paletteOpen = $state(false);
	helpOpen = $state(false);
	contextMenuOpen = $state(false);
	toasts = $state<Toast[]>([]);
	#toastId = 0;

	/** The k9s configuration of the connected context. */
	k9s = $state<K9sConfig | null>(null);
	/** The skin picked for this context: undefined follows k9s, null is Kubri's own colors. */
	skinChoice = $state<string | null | undefined>(undefined);
	/** The skin on screen, and why the wanted one is not. */
	skin = $state<string | null>(null);
	skinError = $state<string | null>(null);
	#skinRequest = 0;
	/** The section the config page scrolls to. */
	configSection = $state<ConfigSection | undefined>(undefined);
	/** A foreground plugin, which gets a terminal over the whole window. */
	pluginTerminal = $state<{ plugin: Plugin; run: PluginRun } | null>(null);
	/** A filter for the resource list to take, from a command like `:pods app=web`. */
	filterRequest = $state<string | null>(null);
	/** Set by the current view: what it has under the cursor. */
	focus: (() => Focus) | null = null;

	/** True while an overlay owns the keyboard. */
	overlayOpen = $derived(
		this.paletteOpen || this.helpOpen || this.contextMenuOpen || this.dialog !== null || this.pluginTerminal !== null
	);

	/** Where plugins run now, which decides the plugins on offer. */
	pluginView = $derived<PluginView>(
		this.page === 'resources' && this.selected
			? { kind: 'resource', resource: this.selected }
			: this.page === 'helm'
				? { kind: 'helm' }
				: { kind: 'other' }
	);

	paletteConfig = $derived<PaletteConfig>({ aliases: this.k9s?.aliases ?? [], skins: this.k9s?.skins ?? [] });

	async load() {
		try {
			const { contexts, cluster } = await backend.contexts();
			this.contexts = contexts;
			if (cluster) {
				await this.connected(cluster);
			} else {
				const fallback = contexts.find((c) => c.isDefault) ?? contexts[0];
				if (fallback) await this.switchContext(fallback.name);
			}
		} catch (err) {
			this.error = errorMessage(err);
		}
	}

	async reloadContexts() {
		try {
			this.contexts = (await backend.reloadContexts()).contexts;
		} catch (err) {
			this.error = errorMessage(err);
		}
	}

	async switchContext(name: string) {
		this.connecting = name;
		this.error = null;
		try {
			await this.connected(await backend.switchContext(name));
		} catch (err) {
			this.error = `Could not connect to ${name}: ${errorMessage(err)}`;
		} finally {
			this.connecting = null;
		}
	}

	/** Shows a resource list, optionally switching namespace, like k9s `:pods kube-system`. */
	open(resource: ResourceInfo, namespace?: string, filter?: string) {
		this.page = 'resources';
		this.selected = resource;
		this.setNamespace(namespace);
		this.details = null;
		if (filter !== undefined) this.filterRequest = filter;
	}

	/** Runs a palette command or hotkey. */
	async run(action: Action) {
		switch (action.type) {
			case 'resource': {
				let resource: ResourceInfo | undefined = action.resource;
				if (action.context && action.context !== this.cluster?.context) {
					await this.switchContext(action.context);
					if (this.cluster?.context !== action.context) return;
					resource = findResource(this.resources, action.resource);
					if (!resource) {
						this.toast(`${action.context} has no ${action.resource.plural}`, 'bad');
						return;
					}
				}
				this.open(resource, action.namespace, action.filter);
				break;
			}
			case 'context':
				if (action.name !== this.cluster?.context) await this.switchContext(action.name);
				break;
			case 'namespace':
				this.namespace = action.name;
				break;
			case 'pulses':
				this.showPulses(action.namespace);
				break;
			case 'xray':
				this.showXray(action.resource, action.namespace);
				break;
			case 'rbac':
				this.showRbac(action.namespace);
				break;
			case 'helm':
				this.showHelm(action.namespace);
				break;
			case 'sanitizer':
				this.showSanitizer(action.namespace);
				break;
			case 'benchmarks':
				this.showBenchmarks();
				break;
			case 'portforwards':
				this.showPortForwards();
				break;
			case 'config':
				this.showConfig(action.section);
				break;
			case 'skin':
				this.setSkin(action.name);
				break;
			case 'help':
				this.helpOpen = true;
				break;
		}
	}

	private setNamespace(namespace: string | undefined) {
		if (namespace !== undefined) this.namespace = namespace === 'all' ? ALL_NAMESPACES : namespace;
	}

	showDetails(
		resource: ResourceInfo,
		target: ObjectRef,
		tab: DetailsTab = 'describe',
		extra: Partial<Pick<Details, 'editing' | 'previous' | 'mode'>> = {}
	) {
		const shown = this.details;
		if (shown && sameResource(shown.resource, resource) && sameObject(shown.target, target)) {
			// Same object: switch tabs in place, so running logs and shells survive.
			Object.assign(shown, { tab, editing: false, ...extra });
			return;
		}
		this.details = { resource, target, tab, editing: false, ...extra };
	}

	/** The cluster overview, optionally switching namespace like `:pulses kube-system`. */
	showPulses(namespace?: string) {
		this.page = 'pulses';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** The XRay trees of `resource`, like k9s `:xray deploy`. */
	showXray(resource: ResourceInfo, namespace?: string) {
		this.page = 'xray';
		this.xrayResource = resource;
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Who may do what, like k9s `:rbac`. */
	showRbac(namespace?: string) {
		this.page = 'rbac';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Helm releases, like k9s `:helm`. */
	showHelm(namespace?: string) {
		this.page = 'helm';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Problems and risky settings, like Popeye; `:popeye kube-system` switches namespace. */
	showSanitizer(namespace?: string) {
		this.page = 'sanitizer';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** HTTP benchmarks, optionally showing one of them. */
	showBenchmarks(id?: number) {
		this.page = 'benchmarks';
		this.details = null;
		if (id !== undefined) this.benchmarkId = id;
		this.refreshBenchmarks();
	}

	async refreshBenchmarks() {
		try {
			this.benchmarks = await backend.benchmarks();
		} catch {
			// Keep the last list; the next refresh tries again.
		}
	}

	/** The k9s configuration: skins, hotkeys, plugins and aliases. */
	showConfig(section?: ConfigSection) {
		this.page = 'config';
		this.details = null;
		this.configSection = section;
	}

	/** Reads the k9s configuration of the connected context and puts on its skin. */
	async loadK9s() {
		const context = this.cluster?.context;
		try {
			const config = await backend.k9sConfig();
			if (this.cluster?.context !== context) return;
			this.k9s = config;
		} catch (err) {
			this.k9s = null;
			this.toast(`Could not read the k9s configuration: ${errorMessage(err)}`, 'bad');
		}
		this.skinChoice = context ? storedSkin(context) : undefined;
		await this.applySkin();
	}

	/** Picks a skin for this context: a name, null for Kubri's colors, undefined to follow k9s. */
	async setSkin(name: string | null | undefined) {
		this.skinChoice = name;
		if (this.cluster) storeSkin(this.cluster.context, name);
		await this.applySkin();
	}

	private async applySkin() {
		const request = ++this.#skinRequest;
		const name = this.skinChoice === undefined ? (this.k9s?.skin ?? null) : this.skinChoice;
		let theme = null;
		let error = null;
		if (name) {
			try {
				theme = skinTheme(await backend.k9sSkin(name), resolveColor);
				if (!theme) error = `The skin ${name} sets no colors Kubri can use.`;
			} catch (err) {
				error = `Could not load the skin ${name}: ${errorMessage(err)}`;
			}
		}
		// A later choice may have finished first.
		if (request !== this.#skinRequest) return;
		applyTheme(theme);
		this.skin = theme ? name : null;
		this.skinError = error;
	}

	showPortForwards() {
		this.page = 'portforwards';
		this.details = null;
		this.refreshForwards();
	}

	async refreshForwards() {
		try {
			this.forwards = await backend.portForwards();
		} catch {
			// Keep the last list; the next refresh tries again.
		}
	}

	toast(text: string, tone: Toast['tone'] = 'ok') {
		const id = ++this.#toastId;
		this.toasts.push({ id, tone, text });
		setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), tone === 'bad' ? 8000 : 4000);
	}

	private async connected(cluster: ClusterInfo) {
		const resources = await backend.resources();
		const previous = this.selected;
		this.cluster = cluster;
		this.resources = resources;
		this.namespace = ALL_NAMESPACES;
		this.details = null;
		this.selected =
			resources.find((r) => previous && sameResource(r, previous)) ??
			resources.find((r) => r.group === '' && r.plural === 'pods') ??
			null;
		const xray = this.xrayResource;
		this.xrayResource = resources.find((r) => xray && sameResource(r, xray)) ?? null;
		if (this.page === 'xray' && !this.xrayResource) this.page = 'resources';
		// Hotkeys, plugins and skins can differ per context.
		this.loadK9s();
	}
}

export function sameResource(a: ResourceRef, b: ResourceRef) {
	return a.group === b.group && a.version === b.version && a.plural === b.plural;
}

export function sameObject(a: ObjectRef, b: ObjectRef) {
	return sameResource(a.resource, b.resource) && a.namespace === b.namespace && a.name === b.name;
}

/** The served resource type `ref` names, ignoring the version if the cluster serves another. */
export function findResource(resources: ResourceInfo[], ref: ResourceRef): ResourceInfo | undefined {
	return (
		resources.find((r) => sameResource(r, ref)) ??
		resources.find((r) => r.group === ref.group && r.plural === ref.plural)
	);
}

export function refOf(resource: ResourceRef): ResourceRef {
	return { group: resource.group, version: resource.version, plural: resource.plural };
}

export const app = new AppState();

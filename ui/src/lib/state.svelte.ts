import { drafts } from './navigation.svelte';
import { access } from './access.svelte';
import {
	backend,
	type AccessCheck,
	type ClusterInfo,
	type BenchInfo,
	type ContextInfo,
	type ContextSettings,
	type Drill,
	type ExplainSubject,
	type Guard,
	type MemoryFix,
	type HelmRelease,
	type K9sConfig,
	type ObjectRef,
	type Plugin,
	type PluginRun,
	type PortForwardInfo,
	type ResourceInfo,
	type ResourceRef,
	type Settings,
	type TableScope,
	type TerminalMode
} from './api';
import { suggest, type Action, type ConfigSection, type PaletteConfig } from './commands';
import { errorMessage } from './format';
import { skinTheme, type Focus, type PluginView } from './k9s';
import type { Menu } from './menu';
import { applyScheme, applyTheme, resolveColor, storeSkin, storedScheme, storedSkin, type Scheme } from './skin';

export const ALL_NAMESPACES = '';

export type DetailsTab = 'describe' | 'events' | 'yaml' | 'logs' | 'shell';

export interface Details {
	target: ObjectRef;
	resource: ResourceInfo;
	tab: DetailsTab;
	editing: boolean;
	/** Start the logs with the previous container instance. */
	previous?: boolean;
	/** How the shell tab connects. */
	mode?: TerminalMode;
	/** The container the shell tab connects to, such as a debug container just started. */
	container?: string;
	/** Show the decoded values of a Secret, like k9s `x`. */
	decoded?: boolean;
}

export type DetailsExtra = Partial<Pick<Details, 'editing' | 'previous' | 'mode' | 'decoded' | 'container'>>;

/** A resource list narrowed to what belongs to one object, like the pods of a Deployment. */
export interface ListScope {
	/** Such as `Pods of Deployment web`. */
	title: string;
	scope: TableScope;
	/** Keeps only what this UID owns. */
	ownerUid: string | null;
}

/** A place in Kubri, for going back and forth like k9s `[`, `]` and `-`. */
interface View {
	page: Page;
	resource: ResourceRef | null;
	namespace: string;
	scope: ListScope | null;
	xray: ResourceRef | null;
}

const HISTORY = 50;

/** Where Kubri 0.2 kept per-context choices in the browser; moved to the settings file once. */
const READ_ONLY_KEY = 'kubri.readonly.';
const ENVIRONMENT_KEY = 'kubri.environment.';
const TOOLS_BESIDE_KEY = 'kubri.toolsBeside';
const PANEL_WIDTH_KEY = 'kubri.panelWidth';

function stored(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

/** Keeps a per-context choice; null forgets it. Without storage it lasts until the page reloads. */
function store(key: string, value: string | null) {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		// Kept in memory only.
	}
}

export const NO_CONTEXT_SETTINGS: ContextSettings = {
	displayName: '',
	label: '',
	color: '',
	production: false,
	readOnly: false,
	locked: false,
	namespace: '',
	agentWrites: false
};

const DEFAULT_SETTINGS: Settings = {
	contexts: {},
	diff: { scale: false, restart: false, image: true, labels: true },
	agents: { http: false, port: 7879, secrets: false },
	explain: { provider: 'none', baseUrl: '', model: '' },
	debug: { image: 'busybox' }
};

/** Per-context choices Kubri 0.2 kept in the browser, as settings, for contexts that have none yet. */
export function legacyContextSettings(
	keys: string[],
	read: (key: string) => string | null,
	existing: Record<string, ContextSettings>
): Record<string, ContextSettings> {
	const found: Record<string, ContextSettings> = {};
	for (const key of keys) {
		const [prefix, name] = key.startsWith(READ_ONLY_KEY)
			? [READ_ONLY_KEY, key.slice(READ_ONLY_KEY.length)]
			: key.startsWith(ENVIRONMENT_KEY)
				? [ENVIRONMENT_KEY, key.slice(ENVIRONMENT_KEY.length)]
				: [null, ''];
		if (!prefix || !name || existing[name]) continue;
		const value = read(key) ?? '';
		const settings = (found[name] ??= { ...NO_CONTEXT_SETTINGS });
		if (prefix === READ_ONLY_KEY) settings.readOnly = value === 'true';
		else {
			settings.label = value.trim();
			settings.production = /\bprod(uction)?\b/i.test(value);
		}
	}
	return found;
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
	| { kind: 'delete'; resource: ResourceInfo; targets: ObjectRef[]; statuses?: Record<string, string> }
	| { kind: 'scale'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'restart'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'labels'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'portforward'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'benchmark'; forward: PortForwardInfo; preset?: BenchInfo }
	| { kind: 'drain'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'helmRollback'; release: HelmRelease; revision: number; done: () => void }
	| { kind: 'helmUninstall'; release: HelmRelease; done: () => void }
	| { kind: 'plugin'; plugin: Plugin; run: PluginRun; variables: Record<string, string> }
	| { kind: 'trigger'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'suspend'; resource: ResourceInfo; target: ObjectRef; suspend: boolean }
	| { kind: 'image'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'rollback'; resource: ResourceInfo; target: ObjectRef }
	/** A debug container in a pod, a debug copy of it, or a shell on a node. */
	| { kind: 'debug'; on: 'pod' | 'node'; target: ObjectRef; way?: 'ephemeral' | 'copy'; container?: string }
	/** A higher memory limit for a container, on the workload that owns its pod. */
	| { kind: 'memory'; resource: ResourceInfo; target: ObjectRef; fix: MemoryFix }
	/** The kubectl commands for an object's actions, to copy. */
	| { kind: 'kubectl'; title: string; commands: { label: string; command: string }[] }
	/** Explains an action RBAC refuses, and what would allow it. */
	| { kind: 'forbidden'; action: string; check: AccessCheck; target: ObjectRef | null };

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
	#namespace = $state(ALL_NAMESPACES);
	get namespace() {
		return this.#namespace;
	}
	/** Switching namespace leaves an unsaved YAML draft only on the user's say, and closes the details. */
	set namespace(value: string) {
		if (value === this.#namespace) return;
		if (drafts.block(() => (this.namespace = value))) return;
		this.#namespace = value;
		this.details = null;
	}
	connecting = $state<string | null>(null);
	error = $state<string | null>(null);

	page = $state<Page>('pulses');
	/** The kind the XRay page starts its trees from. */
	xrayResource = $state<ResourceInfo | null>(null);
	#details = $state<Details | null>(null);
	get details() {
		return this.#details;
	}
	/** Closing or replacing the details asks first while a YAML draft is unsaved. */
	set details(value: Details | null) {
		if (value === this.#details) return;
		if (drafts.block(() => (this.details = value))) return;
		this.#details = value;
	}
	/** The details panel fills the window, sidebar included. */
	maximized = $state(false);
	/** Logs and shells take the place of the list, unless asked to stay beside it. */
	#toolsBeside = $state(stored(TOOLS_BESIDE_KEY) === 'true');
	get toolsBeside() {
		return this.#toolsBeside;
	}
	set toolsBeside(value: boolean) {
		this.#toolsBeside = value;
		store(TOOLS_BESIDE_KEY, value ? 'true' : null);
	}
	/** The width the details panel was dragged to beside the list, or null for the default. */
	#panelWidth = $state(Number(stored(PANEL_WIDTH_KEY)) || null);
	get panelWidth() {
		return this.#panelWidth;
	}
	set panelWidth(value: number | null) {
		this.#panelWidth = value;
		store(PANEL_WIDTH_KEY, value ? String(Math.round(value)) : null);
	}
	/** The details panel takes the place of the list: maximized, or showing logs or a shell. */
	get panelWide(): boolean {
		const tab = this.#details?.tab;
		return !!this.#details && (this.maximized || ((tab === 'logs' || tab === 'shell') && !this.#toolsBeside));
	}
	dialog = $state<Dialog | null>(null);
	forwards = $state<PortForwardInfo[]>([]);
	benchmarks = $state<BenchInfo[]>([]);
	/** The benchmark the benchmarks page shows. */
	benchmarkId = $state<number | null>(null);
	paletteOpen = $state(false);
	helpOpen = $state(false);
	contextMenuOpen = $state(false);
	/** The actions menu that is open, from a right click or a ⋯ or More button. */
	menu = $state<Menu | null>(null);
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
	/** What Explain shows a model, while its panel is open. */
	explaining = $state<ExplainSubject | null>(null);
	/** A filter for the resource list to take, from a command like `:pods app=web`. */
	filterRequest = $state<string | null>(null);
	/** A filter for the RBAC page to take, like `can delete pods` from "Who may do this?". */
	rbacFilterRequest = $state<string | null>(null);
	/** The object the resource list puts its cursor on once it shows up, as after Shift+J. */
	cursorRequest = $state<{ namespace: string | null; name: string } | null>(null);
	/** Narrows the resource list, after Enter on a workload, CronJob or node. */
	scope = $state<ListScope | null>(null);
	#history = $state.raw<View[]>([]);
	#historyAt = $state(-1);
	/** The view before the current one, for `-`. */
	#lastView: View | null = null;
	#restoring = false;
	/** The k9s `defaultView` applies once, when Kubri starts. */
	#startup = true;
	/** Set by the current view: what it has under the cursor. */
	focus: (() => Focus) | null = null;

	/** True while an overlay owns the keyboard. */
	overlayOpen = $derived(
		this.paletteOpen ||
			this.helpOpen ||
			this.contextMenuOpen ||
			this.menu !== null ||
			this.dialog !== null ||
			this.pluginTerminal !== null ||
			this.explaining !== null ||
			drafts.pending !== null
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

	/** Kubri's settings file: names, colors and guard rails per context, and preferences. */
	settings = $state<Settings>(DEFAULT_SETTINGS);
	/** Why the settings file could not be read. */
	settingsError = $state<string | null>(null);
	/** Whether the core lets Kubri change the connected cluster, which it enforces. */
	guard = $state<Guard>({ readOnly: false, reason: null, canUnlock: false, unlockedFor: null });
	/** When an unlocked context locks again, in milliseconds since the epoch. */
	unlockedUntil = $state<number | null>(null);
	#guardTimer: ReturnType<typeof setTimeout> | null = null;
	#guardRequest = 0;
	/** The settings of the connected context. */
	contextSettings = $derived(this.contextSettingsOf(this.cluster?.context));
	/** A label such as "Production" for this context, set by the user; Kubri never guesses it from names. */
	environment = $derived(this.contextSettings.label);
	/** Marked as production by the user: a banner, and typed names before destructive actions. */
	production = $derived(this.contextSettings.production);
	/** k9s `readOnly`, read-only or locked in Kubri: nothing that changes the cluster is offered. */
	readOnly = $derived((this.k9s?.readOnly ?? false) || this.guard.readOnly);
	/** Light or dark picked in Kubri; null follows the system. Skins pick their own. */
	scheme = $state<Scheme>(storedScheme());

	contextSettingsOf(context: string | undefined | null): ContextSettings {
		return (context && this.settings.contexts[context]) || NO_CONTEXT_SETTINGS;
	}

	/** The name to show for a context: the display name set in Kubri, or its own. */
	contextName(context: string): string {
		return this.contextSettingsOf(context).displayName || context;
	}

	async loadSettings() {
		try {
			const reply = await backend.settings();
			this.settings = reply.settings;
			this.settingsError = reply.error;
			if (!reply.error) await this.migrateLegacySettings();
		} catch (err) {
			this.settingsError = errorMessage(err);
		}
	}

	/** Moves the read-only choices and labels of Kubri 0.2 from the browser to the settings file. */
	private async migrateLegacySettings() {
		let keys: string[] = [];
		try {
			keys = Object.keys(localStorage).filter((k) => k.startsWith(READ_ONLY_KEY) || k.startsWith(ENVIRONMENT_KEY));
		} catch {
			return;
		}
		const found = legacyContextSettings(keys, stored, this.settings.contexts);
		for (const [name, settings] of Object.entries(found)) {
			try {
				this.settings = await backend.setContextSettings(name, settings);
			} catch {
				return;
			}
		}
		for (const key of keys) store(key, null);
	}

	/** Saves the settings of a context; the connected one's guard follows. */
	async setContextSettings(context: string, settings: ContextSettings): Promise<boolean> {
		try {
			this.settings = await backend.setContextSettings(context, settings);
			if (context === this.cluster?.context) await this.refreshGuard();
			return true;
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	async setDiffSettings(diff: Settings['diff']) {
		try {
			this.settings = await backend.setDiffSettings(diff);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setAgentSettings(agents: Settings['agents']) {
		try {
			this.settings = await backend.setAgentSettings(agents);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setExplainSettings(explain: Settings['explain']) {
		try {
			this.settings = await backend.setExplainSettings(explain);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async refreshGuard() {
		await this.#askGuard(() => backend.guard(), true);
	}

	/** Asks the core for the guard; only the latest answer counts, so a slow one cannot undo an unlock. */
	async #askGuard(ask: () => Promise<Guard>, quiet = false): Promise<boolean> {
		const request = ++this.#guardRequest;
		try {
			const guard = await ask();
			if (request === this.#guardRequest) this.applyGuard(guard);
			return true;
		} catch (err) {
			// Quietly keep the last state; the core still refuses what it must.
			if (!quiet) this.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	private applyGuard(guard: Guard) {
		this.guard = guard;
		this.unlockedUntil = guard.unlockedFor ? Date.now() + guard.unlockedFor * 1000 : null;
		if (this.#guardTimer) clearTimeout(this.#guardTimer);
		this.#guardTimer = null;
		// Lock again on screen when the core does.
		if (guard.unlockedFor) this.#guardTimer = setTimeout(() => this.refreshGuard(), guard.unlockedFor * 1000 + 500);
	}

	/** Unlocks a locked context for 15 minutes. */
	async unlock() {
		if (drafts.block(() => this.unlock())) return;
		if (await this.#askGuard(() => backend.unlock())) this.toast('Unlocked for 15 minutes');
	}

	async lock() {
		if (drafts.block(() => this.lock())) return;
		await this.#askGuard(() => backend.lock());
	}

	/** The header's lock button: read-only on and off, or a locked context unlocked and locked. */
	toggleReadOnly() {
		if (drafts.block(() => this.toggleReadOnly())) return;
		const context = this.cluster?.context;
		if (!context) return;
		const settings = this.contextSettings;
		if (settings.readOnly) {
			this.setContextSettings(context, { ...settings, readOnly: false });
		} else if (this.guard.canUnlock) {
			if (this.guard.readOnly) this.unlock();
			else this.lock();
		} else {
			this.setContextSettings(context, { ...settings, readOnly: true });
		}
	}

	toggleScheme() {
		const dark = this.scheme ? this.scheme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
		this.scheme = dark ? 'light' : 'dark';
		applyScheme(this.scheme);
	}

	/** Where Kubri is, for the history. */
	view = $derived<View>({
		page: this.page,
		resource: this.selected && refOf(this.selected),
		namespace: this.namespace,
		scope: this.scope,
		xray: this.xrayResource && refOf(this.xrayResource)
	});

	async load() {
		try {
			await this.loadSettings();
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

	/** Reads the kubeconfig again. `quiet` keeps the old list on errors, for refreshes nobody asked for. */
	async reloadContexts(quiet = false) {
		try {
			this.contexts = (await backend.reloadContexts()).contexts;
		} catch (err) {
			if (!quiet) this.error = errorMessage(err);
		}
	}

	async switchContext(name: string) {
		if (drafts.block(() => this.switchContext(name))) return;
		// A dialog was reviewed against the current context; it must not run against the next one.
		if (this.dialog || this.connecting) {
			this.toast('Finish the current action before switching contexts', 'bad');
			return;
		}
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
	open(resource: ResourceInfo, namespace?: string, filter?: string, scope: ListScope | null = null) {
		if (drafts.block(() => this.open(resource, namespace, filter, scope))) return;
		this.page = 'resources';
		this.selected = resource;
		this.setNamespace(namespace);
		this.details = null;
		this.scope = scope;
		if (filter !== undefined) this.filterRequest = filter;
	}

	/** Follows Enter from an object to the list of what belongs to it. */
	drill(drill: Drill) {
		const resource = findResource(this.resources, drill.resource);
		if (!resource) {
			this.toast(`The cluster does not serve ${drill.resource.plural}`, 'bad');
			return;
		}
		const scope = { title: drill.title, scope: drill.scope, ownerUid: drill.ownerUid };
		this.open(resource, drill.namespace ?? 'all', '', scope);
	}

	/** Shows the list of `resource` with the cursor on one object, as k9s does for owners. */
	openAt(resource: ResourceInfo, namespace: string | null, name: string) {
		const keep = resource.namespaced && namespace && this.namespace !== ALL_NAMESPACES;
		this.open(resource, keep ? namespace : undefined, '');
		this.cursorRequest = { namespace, name };
	}

	/** Records a view in the history, unless going back or forth brought it up. */
	remember(view: View) {
		if (this.#restoring) {
			this.#restoring = false;
			return;
		}
		const current = this.#history[this.#historyAt];
		if (current && sameView(current, view)) return;
		if (current) this.#lastView = current;
		this.#history = [...this.#history.slice(0, this.#historyAt + 1), view].slice(-HISTORY);
		this.#historyAt = this.#history.length - 1;
	}

	/** Goes back (`[`) or forward (`]`) in the history; false when there is nowhere to go. */
	get canGoBack() {
		return this.#historyAt > 0;
	}

	get canGoForward() {
		return this.#historyAt < this.#history.length - 1;
	}

	go(delta: -1 | 1): boolean {
		if (drafts.block(() => this.go(delta))) return false;
		const at = this.#historyAt + delta;
		const view = this.#history[at];
		if (!view) return false;
		this.#lastView = this.#history[this.#historyAt] ?? null;
		this.#historyAt = at;
		this.restore(view);
		return true;
	}

	/** Returns to the view before this one, and back again (`-`). */
	toggleLast(): boolean {
		if (drafts.block(() => this.toggleLast())) return false;
		const last = this.#lastView;
		if (!last) return false;
		const at = this.#history.findLastIndex((v) => sameView(v, last));
		this.#lastView = this.#history[this.#historyAt] ?? null;
		if (at !== -1) this.#historyAt = at;
		this.restore(last);
		return true;
	}

	private restore(view: View) {
		const resource = view.resource && findResource(this.resources, view.resource);
		const xray = view.xray && findResource(this.resources, view.xray);
		this.#restoring = !sameView(view, this.view);
		this.page = view.page;
		if (resource) this.selected = resource;
		if (xray) this.xrayResource = xray;
		this.namespace = view.namespace;
		this.scope = view.scope;
		this.details = null;
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

	showDetails(resource: ResourceInfo, target: ObjectRef, tab: DetailsTab = 'describe', extra: DetailsExtra = {}) {
		if (drafts.block(() => this.showDetails(resource, target, tab, extra))) return;
		const shown = this.details;
		if (shown && sameResource(shown.resource, resource) && sameObject(shown.target, target)) {
			// Same object: switch tabs in place, so running logs and shells survive.
			Object.assign(shown, { tab, editing: false, ...extra });
			return;
		}
		this.details = { resource, target: { ...target, context: this.cluster?.context }, tab, editing: false, ...extra };
	}

	/** The cluster overview, optionally switching namespace like `:pulses kube-system`. */
	showPulses(namespace?: string) {
		if (drafts.block(() => this.showPulses(namespace))) return;
		this.page = 'pulses';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** The XRay trees of `resource`, like k9s `:xray deploy`. */
	showXray(resource: ResourceInfo, namespace?: string) {
		if (drafts.block(() => this.showXray(resource, namespace))) return;
		this.page = 'xray';
		this.xrayResource = resource;
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Who may do what, like k9s `:rbac`. */
	showRbac(namespace?: string) {
		if (drafts.block(() => this.showRbac(namespace))) return;
		this.page = 'rbac';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Helm releases, like k9s `:helm`. */
	showHelm(namespace?: string) {
		if (drafts.block(() => this.showHelm(namespace))) return;
		this.page = 'helm';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** Problems and risky settings, like Popeye; `:popeye kube-system` switches namespace. */
	showSanitizer(namespace?: string) {
		if (drafts.block(() => this.showSanitizer(namespace))) return;
		this.page = 'sanitizer';
		this.details = null;
		this.setNamespace(namespace);
	}

	/** HTTP benchmarks, optionally showing one of them. */
	showBenchmarks(id?: number) {
		if (drafts.block(() => this.showBenchmarks(id))) return;
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
		if (drafts.block(() => this.showConfig(section))) return;
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
			// The k9s files can turn read-only on or off.
			this.refreshGuard();
			// k9s opens its `defaultView` when it starts.
			const start = this.#startup && config.defaultView;
			this.#startup = false;
			if (start) {
				const [first] = suggest(start, this.resources, this.contexts, this.namespaces, this.paletteConfig);
				if (first) await this.run(first.action);
			}
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
		if (drafts.block(() => this.showPortForwards())) return;
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
		this.details = null;
		this.resources = [];
		this.cluster = null;
		const resources = await backend.resources();
		const previous = this.selected;
		this.cluster = cluster;
		this.k9s = null;
		access.reset(cluster.context);
		this.#guardRequest++;
		this.applyGuard({ readOnly: true, reason: 'Kubri is still checking whether it may change this cluster', canUnlock: false, unlockedFor: null });
		this.refreshGuard();
		this.resources = resources;
		this.namespace = this.contextSettingsOf(cluster.context).namespace || ALL_NAMESPACES;
		this.scope = null;
		// Views of the last cluster may not exist in this one.
		this.#history = [];
		this.#historyAt = -1;
		this.#lastView = null;
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

function sameRef(a: ResourceRef | null, b: ResourceRef | null) {
	return a === b || (!!a && !!b && sameResource(a, b));
}

function sameView(a: View, b: View) {
	return (
		a.page === b.page &&
		a.namespace === b.namespace &&
		sameRef(a.resource, b.resource) &&
		sameRef(a.xray, b.xray) &&
		JSON.stringify(a.scope) === JSON.stringify(b.scope)
	);
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

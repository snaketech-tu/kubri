import type { Fill } from './resources';
import { tick } from 'svelte';
import { drafts } from './navigation.svelte';
import { access } from './access.svelte';
import { creating } from './create.svelte';
import type { TemplateParams } from './create';
import type { ChangesFilter } from './changes';
import { dock } from './dock.svelte';
import { reach } from './reach.svelte';
import {
	backend,
	type AccessCheck,
	type ClusterInfo,
	type UpdateCheck,
	type ColumnSettings,
	type FavoriteForward,
	type FavoriteStart,
	type GitOpsAction,
	type BenchInfo,
	type ContextInfo,
	type ContextSettings,
	type Drill,
	type ExplainSubject,
	type Guard,
	type MemoryFix,
	type HelmRelease,
	type IntegrationId,
	type Integrations,
	type K9sConfig,
	type ObjectRef,
	type Plugin,
	type PluginRun,
	type PortForwardInfo,
	type ResourceInfo,
	type ResourceRef,
	type SavedSearch,
	type Settings,
	type TableScope,
	type TerminalMode,
	type KindView,
	type ViewSet
} from './api';
import { suggest, type Action, type ConfigSection, type PaletteConfig } from './commands';
import { errorMessage } from './format';
import { plural, t } from './i18n';
import { coreText } from './i18n/errors';
import { feedbackLink } from './feedback';
import { readOnlyReason } from './guard';
import { applyZoom, storedZoom } from './appearance';
import { dismissRelease, dismissedRelease, releaseChecks, setReleaseChecks } from './releases';
import { loadPins, savePins, samePin, toggled as togglePinIn, type Pin } from './pins';
import { Places, type Place } from './places';
import { fromHash, toHash, type Address } from './address';
import { keepViews, lastContext, rememberContext, saveView, savedView, startContext, startSetting } from './startup';
import { recentObjects, remember, type RecentObject } from './search';
import { skinTheme, type Focus, type PluginView } from './k9s';
import type { Menu, MenuAction } from './menu';
import { applyScheme, applyTheme, resolveColor, storeSkin, storedScheme, storedSkin, type Scheme } from './skin';
import { xrayResources } from './xray';
import { productionRule, withMarked, type ProductionRule } from './production';
import type { ToolId } from './nav';

export const ALL_NAMESPACES = '';

export type DetailsTab = 'describe' | 'events' | 'yaml' | 'metrics' | 'logs' | 'shell';

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
const SIDEBAR_WIDTH_KEY = 'kubri.sidebarWidth';

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
	productionNamespaces: [],
	productionLabels: [],
	readOnly: false,
	locked: false,
	namespace: '',
	namespaces: [],
	namespaceSelector: '',
	agentWrites: false,
	forwards: [],
	integrations: { prometheus: '', grafana: '', grafanaToken: '', loki: '', grafanaLogs: '', labels: '' }
};

const DEFAULT_SETTINGS: Settings = {
	contexts: {},
	diff: { scale: false, restart: false, image: true, labels: true },
	agents: { http: false, port: 7879, secrets: false },
	explain: { provider: 'none', baseUrl: '', model: '' },
	debug: { image: 'busybox' },
	columns: {},
	searches: [],
	history: { record: true, keepDays: 14, maxMb: 200 },
	connections: { keepRecent: 3 },
	terminal: { shell: '', args: [], startDir: '', fontSize: 13, fontFamily: '' },
	labelTemplates: []
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

export const PAGES = [
	'resources',
	'portforwards',
	'benchmarks',
	'pulses',
	'xray',
	'graph',
	'rbac',
	'helm',
	'create',
	'sanitizer',
	'integrations',
	'rightsizing',
	'fleet',
	'search',
	'changes',
	'config',
	'tools',
	'kinds'
] as const;

export type Page = (typeof PAGES)[number];

export type Dialog =
	| { kind: 'delete'; resource: ResourceInfo; targets: ObjectRef[]; statuses?: Record<string, string> }
	| { kind: 'scale'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'restart'; resource: ResourceInfo; target: ObjectRef }
	/** Several marked objects at once. */
	| { kind: 'scaleMany'; resource: ResourceInfo; targets: ObjectRef[] }
	| { kind: 'restartMany'; resource: ResourceInfo; targets: ObjectRef[] }
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
	| { kind: 'rollback'; resource: ResourceInfo; target: ObjectRef; owner?: { kind: string; plural: string; name: string }; revision?: number }
	/** A Flux or Argo CD action; `autoSync` turns Argo CD's auto-sync off or on, as it is now. */
	| { kind: 'gitops'; resource: ResourceInfo; target: ObjectRef; action: GitOpsAction | 'autoSync' }
	/**
	 * The requests and limits of a workload's containers, with the room on its nodes. `fill`
	 * prefills values, as right-sizing advice suggests them; the cursor starts at the first.
	 */
	| {
			kind: 'resources';
			resource: ResourceInfo;
			target: ObjectRef;
			fill?: Fill[];
			/** Why, as the advice says it. */
			note?: string;
	  }
	/** Has cert-manager issue a Certificate again now. */
	| { kind: 'renew'; resource: ResourceInfo; target: ObjectRef }
	/** A debug container in a pod, a debug copy of it, or a shell on a node. */
	| { kind: 'debug'; on: 'pod' | 'node'; target: ObjectRef; way?: 'ephemeral' | 'copy'; container?: string }
	/** The same object in another context or namespace, side by side. */
	| { kind: 'compare'; resource: ResourceInfo; target: ObjectRef }
	/** The Grafana dashboards and the context's links for an object. */
	| { kind: 'grafana'; target: ObjectRef }
	/** A higher memory limit for a container, on the workload that owns its pod. */
	| { kind: 'memory'; resource: ResourceInfo; target: ObjectRef; fix: MemoryFix }
	/** The kubectl commands for an object's actions, to copy. */
	| { kind: 'kubectl'; title: string; commands: { label: string; command: string }[] }
	/** Explains an action RBAC refuses, and what would allow it. */
	| { kind: 'forbidden'; action: string; check: AccessCheck; target: ObjectRef | null }
	/** Saves a namespace or the cluster to a snapshot file, or opens one; `namespace` null is the cluster. */
	| { kind: 'snapshot'; namespace: string | null };

let activityId = 0;

/** One change made in this session. */
export interface Activity {
	id: number;
	at: number;
	context: string;
	text: string;
	ok: boolean;
	/** Sets things back, where Kubernetes allows it. */
	undo?: () => Promise<unknown>;
	undone: boolean;
}

export interface Toast {
	id: number;
	tone: 'ok' | 'bad';
	text: string;
}

/** Global app state: kubeconfig contexts, the connected cluster and what is being browsed. */
class AppState {
	contexts = $state<ContextInfo[]>([]);
	/** Contexts kept connected in the background, most recent first. */
	warm = $state<string[]>([]);
	/** The context Kubri was on before this one, for `:ctx -`. */
	previousContext = $state<string | null>(null);
	/**
	 * The kubeconfig's `current-context`, when Kubri started in another one it used last and that
	 * one did not answer: the connecting card offers it instead.
	 */
	startFallback = $state<string | null>(null);
	/** Where each context was left, to return there. */
	#places = new Places<View>();
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
	/** When the connect under way began, for the time it shows. */
	connectingSince = $state(0);
	/** Counts connects, so that one cancelled or overtaken is ignored when it ends. */
	#connectAttempt = 0;
	/** A cancel on its way to the backend, which the next connect waits for so it is not cancelled too. */
	#cancelling: Promise<void> | null = null;
	error = $state<string | null>(null);

	page = $state<Page>('pulses');
	/** How many rows the list shows, null without a list. */
	listRows = $state<number | null>(null);
	/** What the search page searches for, as typed. */
	searchQuery = $state('');
	/** What the What changed page shows; replaced as a whole when opened from elsewhere. */
	changesFilter = $state<ChangesFilter>({});
	/** The kind the Topology trees start from. */
	xrayResource = $state<ResourceInfo | null>(null);
	/** What the All kinds page starts filtered by, as `:kinds cert` asks. */
	kindsFilter = $state('');
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
	/** The width the sidebar was dragged to, or null for the default. */
	#sidebarWidth = $state(Number(stored(SIDEBAR_WIDTH_KEY)) || null);
	get sidebarWidth() {
		return this.#sidebarWidth;
	}
	set sidebarWidth(value: number | null) {
		this.#sidebarWidth = value;
		store(SIDEBAR_WIDTH_KEY, value ? String(Math.round(value)) : null);
	}
	/** The details panel takes the place of the list: maximized, or showing logs or a shell. */
	get panelWide(): boolean {
		const tab = this.#details?.tab;
		return !!this.#details && (this.maximized || ((tab === 'logs' || tab === 'shell') && !this.#toolsBeside));
	}
	dialog = $state<Dialog | null>(null);
	/** The dialog that offers to install the new `release`. */
	updateOpen = $state(false);
	forwards = $state<PortForwardInfo[]>([]);
	benchmarks = $state<BenchInfo[]>([]);
	/** The benchmark the benchmarks page shows. */
	benchmarkId = $state<number | null>(null);
	paletteOpen = $state(false);
	helpOpen = $state(false);

	/** The first-run tour; shown once, and again from the help. */
	tourOpen = $state(false);

	markTourSeen() {
		try {
			localStorage.setItem('kubri.tour', 'seen');
		} catch {
			// The tour then shows again next time; no harm.
		}
	}

	/** Shows the tour the first time Kubri has a cluster to show. */
	maybeStartTour() {
		try {
			if (localStorage.getItem('kubri.tour') === 'seen') return;
		} catch {
			return;
		}
		this.tourOpen = true;
	}

	/** Every change made from here in this session, newest first. */
	activity = $state.raw<Activity[]>([]);
	activityOpen = $state(false);

	/** Adds a change to the session activity; an entry with `undo` can be taken back. */
	record(entry: Omit<Activity, 'id' | 'at' | 'context' | 'undone'>): number {
		const id = ++activityId;
		this.activity = [{ ...entry, id, at: Date.now(), context: this.cluster?.context ?? '', undone: false }, ...this.activity].slice(0, 200);
		return id;
	}

	/** Takes back an entry that can be, and records that it was. */
	async undoActivity(id: number) {
		const entry = this.activity.find((a) => a.id === id);
		if (!entry?.undo || entry.undone) return;
		try {
			await entry.undo();
			this.activity = this.activity.map((a) => (a.id === id ? { ...a, undone: true } : a));
			this.toast(t('Undone: {what}', { what: entry.text }));
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}
	contextMenuOpen = $state(false);
	/** The dialog listing the kubeconfig files Kubri reads, where more can be loaded; 'copy' opens
	 * straight to adding contexts from a file, as the browser's start page does. */
	kubeconfigFilesOpen = $state<boolean | 'copy'>(false);
	/** Why Kubri has no contexts to connect to, such as a missing kubeconfig; null when it has some. */
	kubeconfigProblem = $state<string | null>(null);
	/** The last failed connection, for the start page to say what to try. */
	connectFailure = $state<{ context: string; error: string } | null>(null);
	/** The actions menu that is open, from a right click or a ⋯ or More button. */
	menu = $state<Menu | null>(null);
	toasts = $state<Toast[]>([]);
	#toastId = 0;

	/** The k9s configuration of the connected context. */
	k9s = $state<K9sConfig | null>(null);
	/** The tools the connected cluster runs, like Prometheus or Flux. */
	integrations = $state<Integrations | null>(null);
	/** How custom resource kinds show: columns, health, details and actions from YAML files. */
	views = $state<ViewSet | null>(null);
	/** The skin picked for this context: undefined follows k9s, null is Kubri's own colors. */
	skinChoice = $state<string | null | undefined>(undefined);
	/** The skin on screen, and why the wanted one is not. */
	skin = $state<string | null>(null);
	skinError = $state<string | null>(null);
	/** Whether the skin on screen is dark. */
	skinDark = $state(false);
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
	/** The view the web app's address named when it opened, for the first context it connects to. */
	#addressed: Address<Page, ListScope> | null = null;
	/**
	 * True until the first context's k9s files are read, so the overview does not open, and list
	 * every pod for its counts, just before the `defaultView` replaces it.
	 */
	starting = $state(true);
	/** Whether the first look at the kubeconfig is done, so the start page does not flash before it. */
	loaded = $state(false);
	/** Set by the current view: what it has under the cursor. */
	focus: (() => Focus) | null = null;

	/** True while an overlay owns the keyboard. */
	overlayOpen = $derived(
		this.paletteOpen ||
			this.helpOpen ||
			this.tourOpen ||
			this.contextMenuOpen ||
			this.connecting !== null ||
			this.kubeconfigFilesOpen ||
			this.menu !== null ||
			this.dialog !== null ||
			this.updateOpen ||
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

	/** Whether local terminals are on: always in the desktop app; the web app asks the server. */
	localTerminal = $state(backend.desktop);
	#localTerminalAsked = false;
	paletteConfig = $derived<PaletteConfig>({ aliases: this.k9s?.aliases ?? [], skins: this.k9s?.skins ?? [], terminal: this.localTerminal });

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
	/** The labels of the namespaces of the connected context, where Kubri may list them. */
	namespaceLabels = $state<Record<string, Record<string, string>>>({});
	/** The namespace picked is marked as production, in a context that is not. */
	productionNamespace = $derived(!this.contextSettings.production && !!this.namespace && !!this.productionRuleOf(this.namespace));

	/** What marks `namespace` of the connected context as production, in a context that is not; null if nothing does. */
	productionRuleOf(namespace: string | null | undefined, labels?: Record<string, string>): ProductionRule | null {
		if (!namespace || this.contextSettings.production) return null;
		return productionRule(this.contextSettings, namespace, labels ?? this.namespaceLabels[namespace]);
	}

	/** Whether changes in `namespace` get production's care: the context, or the namespace, is marked. */
	productionIn(namespace: string | null | undefined): boolean {
		return this.production || !!this.productionRuleOf(namespace);
	}

	/** Marks namespaces of the connected context as production one by one, or no longer. */
	async setProductionNamespace(namespaces: string | string[], production: boolean) {
		const context = this.cluster?.context;
		// A name a pattern or label rule already marks needs no entry of its own.
		const names = (typeof namespaces === 'string' ? [namespaces] : namespaces).filter((n) => !production || !this.productionRuleOf(n));
		if (!context || !names.length) return;
		const settings = this.contextSettingsOf(context);
		if (await this.setContextSettings(context, { ...settings, productionNamespaces: withMarked(settings, names, production) })) {
			const name = names.join(', ');
			const n = names.length;
			if (n > 1) {
				this.toast(production ? plural(n, '{n} namespace is now production', '{n} namespaces are now production') : plural(n, '{n} namespace is no longer production', '{n} namespaces are no longer production'));
			}
			else this.toast(production ? t('{name} is now a production namespace', { name }) : t('{name} is no longer a production namespace', { name }));
		}
	}
	/** k9s `readOnly`, read-only or locked in Kubri: nothing that changes the cluster is offered. */
	readOnly = $derived((this.k9s?.readOnly ?? false) || this.guard.readOnly);
	/** Light or dark picked in Kubri; null follows the system. Skins pick their own. */
	scheme = $state<Scheme>(storedScheme());
	/** Whether the system is set to dark; the page follows its changes. */
	systemDark = $state(typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches);
	/** Whether the colors on screen are dark now, a skin's or Kubri's own. */
	dark = $derived(this.skin ? this.skinDark : this.scheme ? this.scheme === 'dark' : this.systemDark);

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

	/** Replaces the label templates for logs. */
	async setLabelTemplates(templates: Settings['labelTemplates']): Promise<boolean> {
		try {
			this.settings = await backend.setLabelTemplates(templates);
			return true;
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	/** Replaces the searches kept in the sidebar. */
	async setSavedSearches(searches: SavedSearch[]): Promise<boolean> {
		try {
			this.settings = await backend.setSavedSearches(searches);
			return true;
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	/** Saves how lists show a kind. */
	async setColumnSettings(kind: string, columns: ColumnSettings) {
		try {
			this.settings = await backend.setColumnSettings(kind, columns);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setDiffSettings(diff: Settings['diff']) {
		try {
			this.settings = await backend.setDiffSettings(diff);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setConnectionSettings(connections: Settings['connections']) {
		try {
			this.settings = await backend.setConnectionSettings(connections);
			this.refreshWarm();
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setTerminalSettings(terminal: Settings['terminal']) {
		try {
			this.settings = await backend.setTerminalSettings(terminal);
		} catch (err) {
			this.toast(errorMessage(err), 'bad');
		}
	}

	async setHistorySettings(history: Settings['history']) {
		try {
			this.settings = await backend.setHistorySettings(history);
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
		if (await this.#askGuard(() => backend.unlock())) this.toast(t('Unlocked for 15 minutes'));
	}

	async lock() {
		if (drafts.block(() => this.lock())) return;
		await this.#askGuard(() => backend.lock());
	}

	/** Says why an action that changes the cluster did nothing, and how to allow it where that is possible. */
	explainReadOnly(action: string) {
		const what = action.replace(/…$/, '');
		this.toast(t('{action} is not available: {reason}', { action: what, reason: readOnlyReason(this.guard, this.contextSettings.readOnly, this.k9s?.readOnly ?? false) }), 'bad');
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

	/** Switches to Kubri's other colors; with a skin on screen, takes it off for this context. */
	toggleScheme() {
		this.setScheme(this.dark ? 'light' : 'dark');
		if (this.skin) void this.setSkin(null);
	}

	/** Light, dark, or null to follow the system. */
	setScheme(scheme: Scheme) {
		this.scheme = scheme;
		applyScheme(scheme);
	}

	/** The desktop app's zoom level, kept for the next start. */
	zoom = $state(storedZoom());

	setZoom(zoom: number) {
		this.zoom = zoom;
		applyZoom(zoom).catch((err) => this.toast(t('Could not zoom: {error}', { error: errorMessage(err) }), 'bad'));
	}

	/** Whether Kubri asks GitHub for new releases when it starts. */
	releaseChecks = $state(releaseChecks());
	/** A release newer than this one, until the user waves it away. */
	release = $state<UpdateCheck | null>(null);

	setReleaseChecks(on: boolean) {
		this.releaseChecks = on;
		setReleaseChecks(on);
		if (on) this.checkRelease();
		else this.release = null;
	}

	/** With `offer`, as when Kubri starts, opens the update dialog if Kubri can install the release. */
	async checkRelease(offer = false) {
		if (!this.releaseChecks) return;
		try {
			const latest = await backend.checkUpdate();
			this.release = latest.newer && latest.version !== dismissedRelease() ? latest : null;
			if (offer && this.release?.installable) this.updateOpen = true;
		} catch {
			// Offline or rate-limited: no notice, and the next start asks again.
		}
	}

	dismissRelease() {
		if (this.release) dismissRelease(this.release.version);
		this.release = null;
		this.updateOpen = false;
	}

	/** Where Kubri is, for the history. */
	view = $derived<View>({
		page: this.page,
		resource: this.selected && refOf(this.selected),
		namespace: this.namespace,
		scope: this.scope,
		xray: this.xrayResource && refOf(this.xrayResource)
	});

	/** The view as the web app's address, so a reload opens it again; none for a snapshot. */
	address = $derived.by(() => {
		if (!this.cluster || this.cluster.snapshot) return null;
		const { page, resource, namespace, scope, xray } = this.view;
		// Other pages remember the list to go back to, but it is not where they are.
		const list = page === 'resources';
		return toHash({
			context: this.cluster.context,
			page,
			resource: list ? resource : null,
			namespace,
			scope: list ? scope : null,
			xray: page === 'xray' ? xray : null
		});
	});

	/** Reads the view to open from the address the web app was opened with. */
	readAddress(hash: string) {
		this.#addressed = fromHash<Page, ListScope>(hash, PAGES);
	}

	/** Opens the view of an address typed or pasted into the address bar, in the connected context. */
	goToAddress(hash: string) {
		if (!this.cluster || this.cluster.snapshot || hash === this.address) return;
		if (drafts.block(() => this.goToAddress(hash))) {
			// Staying for the unsaved changes: the address says so too.
			if (this.address) history.replaceState(history.state, '', this.address);
			return;
		}
		this.readAddress(hash);
		this.openAddressed(this.cluster.context);
		this.#addressed = null;
	}

	async load() {
		this.checkRelease(true);
		try {
			await this.loadSettings();
			let list: Awaited<ReturnType<typeof backend.contexts>>;
			try {
				list = await backend.contexts();
			} catch (err) {
				// The core's own words: errorMessage would translate them.
				const message = err instanceof Error ? err.message : String(err);
				const prefix = 'no kubeconfig loaded: ';
				if (!message.startsWith(prefix)) throw err;
				this.contexts = [];
				const problem = coreText(message.slice(prefix.length));
				this.kubeconfigProblem = problem.charAt(0).toUpperCase() + problem.slice(1);
				return;
			}
			const { contexts, cluster, warm } = list;
			this.contexts = contexts;
			this.warm = warm ?? [];
			this.kubeconfigProblem = contexts.length ? null : t('Your kubeconfig has no contexts.');
			if (contexts.length) keepViews(contexts.map((c) => c.name));
			if (cluster) {
				await this.connected(cluster);
			} else {
				const start = startContext(contexts, lastContext(), startSetting(), this.#addressed?.context);
				if (start) {
					const current = contexts.find((c) => c.isDefault)?.name ?? contexts[0]?.name ?? null;
					await this.switchContext(start);
					this.startFallback = !this.cluster && current && current !== start ? current : null;
				}
			}
		} catch (err) {
			this.error = errorMessage(err);
		} finally {
			this.loaded = true;
		}
	}

	/** Reads the kubeconfig again. `quiet` keeps the old list on errors, for refreshes nobody asked for. */
	async reloadContexts(quiet = false) {
		try {
			const list = await backend.reloadContexts();
			this.contexts = list.contexts;
			if (this.contexts.length) this.kubeconfigProblem = null;
			this.warm = list.warm ?? [];
			this.#places.keepOnly(list.contexts.map((c) => c.name));
			if (list.contexts.length) keepViews(list.contexts.map((c) => c.name));
			const current = this.cluster?.context;
			dock.keepContexts([...list.contexts.map((c) => c.name), ...(current ? [current] : [])]);
		} catch (err) {
			if (!quiet) this.error = errorMessage(err);
		}
	}

	/** What Kubri is connecting to, or the snapshot file it is opening, in words. */
	connectingText = $derived(
		this.connecting === null
			? null
			: this.contexts.some((c) => c.name === this.connecting)
				? t('Connecting to {name}…', { name: this.connecting })
				: t('Opening {name}…', { name: this.connecting })
	);

	/** The snapshot open instead of a live cluster. */
	snapshot = $derived(this.cluster?.snapshot ?? null);

	/** The time ages count to: now, or when the open snapshot was taken. */
	now(): number {
		return this.snapshot?.taken ?? Date.now();
	}

	/** Opens the snapshot dialog, for the current namespace unless `namespace` says otherwise. */
	showSnapshot(namespace?: string | null) {
		const current = this.namespace === ALL_NAMESPACES ? null : this.namespace;
		this.dialog = { kind: 'snapshot', namespace: namespace === undefined ? current : namespace };
	}

	/** Saves a namespace, or the whole cluster, to a snapshot file. */
	async takeSnapshot(namespace: string | null) {
		this.toast(namespace ? t('Saving a snapshot of namespace {name}…', { name: namespace }) : t('Saving a snapshot of the cluster…'));
		try {
			const path = await backend.takeSnapshot(namespace);
			if (path) this.toast(t('Saved {name}', { name: path }));
		} catch (err) {
			this.toast(t('Could not save the snapshot: {error}', { error: errorMessage(err) }), 'bad');
		}
	}

	/** Opens a snapshot file in place of the cluster; switching context leaves it. */
	async openSnapshot(file: File) {
		if (drafts.block(() => this.openSnapshot(file))) return;
		if (this.dialog || this.connecting) {
			this.toast(t('Finish the current action before opening a snapshot'), 'bad');
			return;
		}
		this.connecting = file.name;
		this.error = null;
		try {
			await this.connected(await backend.openSnapshot(file));
		} catch (err) {
			this.error = t('Could not open {name}: {error}', { name: file.name, error: errorMessage(err) });
		} finally {
			this.connecting = null;
		}
	}

	/** Leaves a snapshot for the context it was taken from, if this kubeconfig has it, or the default one. */
	leaveSnapshot() {
		const from = this.snapshot?.context;
		const back = this.contexts.find((c) => c.name === from) ?? this.contexts.find((c) => c.isDefault) ?? this.contexts[0];
		if (back) this.switchContext(back.name);
		else this.contextMenuOpen = true;
	}

	async switchContext(name: string) {
		if (drafts.block(() => this.switchContext(name))) return;
		// A dialog was reviewed against the current context; it must not run against the next one.
		if (this.dialog || this.connecting) {
			this.toast(t('Finish the current action before switching contexts'), 'bad');
			return;
		}
		// The view stays, dimmed, while connecting; it keeps its place for when it comes back.
		this.savePlace();
		const attempt = ++this.#connectAttempt;
		this.connecting = name;
		this.connectingSince = Date.now();
		this.error = null;
		this.connectFailure = null;
		try {
			await this.#cancelling;
			const cluster = await backend.switchContext(name);
			if (attempt !== this.#connectAttempt) return;
			reach.connected(name, cluster.server);
			await this.connected(cluster);
			this.refreshWarm();
		} catch (err) {
			if (attempt !== this.#connectAttempt) return;
			const server = this.contexts.find((c) => c.name === name)?.server ?? null;
			reach.failed(name, errorMessage(err), server);
			// The connecting card says what failed and what to try.
			this.connectFailure = { context: name, error: errorMessage(err) };
		} finally {
			if (attempt === this.#connectAttempt) this.connecting = null;
		}
	}

	/** Stops waiting for the context being connected to, and stays where Kubri was. */
	cancelConnect() {
		const name = this.connecting;
		if (name === null || !this.contexts.some((c) => c.name === name)) return;
		this.#connectAttempt++;
		this.connecting = null;
		const cancelling = backend.cancelSwitch().catch(() => {});
		this.#cancelling = cancelling;
		void cancelling.then(() => {
			if (this.#cancelling === cancelling) this.#cancelling = null;
		});
		this.toast(
			this.cluster
				? t('Stopped connecting to {name}; still on {current}', { name, current: this.contextName(this.cluster.context) })
				: t('Stopped connecting to {name}', { name })
		);
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
			this.toast(t('The cluster does not serve {kind}', { kind: drill.resource.plural }), 'bad');
			return;
		}
		const scope = { title: drill.title, scope: drill.scope, ownerUid: drill.ownerUid };
		const before = this.namespace;
		this.open(resource, drill.namespace ?? 'all', '', scope);
		// What belongs to an object lives in its namespace, so the list needs it. Say so, and how to undo it.
		if (drill.namespace && before !== drill.namespace) {
			this.toast(
				before === ALL_NAMESPACES
					? t('Showing {name} only. Back returns to all namespaces.', { name: drill.namespace })
					: t('Showing {name} only. Back returns to {before}.', { name: drill.namespace, before })
			);
		}
	}

	/** Shows the list of `resource` with the cursor on one object, as k9s does for owners. */
	openAt(resource: ResourceInfo, namespace: string | null, name: string) {
		const keep = resource.namespaced && namespace && this.namespace !== ALL_NAMESPACES;
		this.open(resource, keep ? namespace : undefined, '');
		this.cursorRequest = { namespace, name };
	}

	/** Records a view in the history, unless going back or forth brought it up. */
	remember(view: View) {
		// For the next start, too.
		const cluster = this.cluster;
		if (cluster && !cluster.snapshot) saveView(cluster.context, { page: view.page, resource: view.resource, namespace: view.namespace, xray: view.xray });
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

	/**
	 * Shows a context's remembered place again. A kind the cluster no longer serves leaves the
	 * defaults chosen on connecting.
	 */
	private returnTo({ view, history, historyAt, filter, cursor }: Place<View>, shown: View) {
		const resource = view.resource && findResource(this.resources, view.resource);
		if (view.resource && !resource) return;
		const xray = view.xray && findResource(this.resources, view.xray);
		if (view.page === 'xray' && !xray) return;
		this.page = view.page;
		if (resource) this.selected = resource;
		if (xray) this.xrayResource = xray;
		this.namespace = view.namespace;
		this.scope = view.scope;
		this.#history = history;
		this.#historyAt = historyAt;
		// The history already holds this view; the effect that records views must skip it.
		this.#restoring = !sameView(view, shown);
		// For the list of this context, which mounts after this update; the one leaving must
		// not take them.
		const context = this.cluster?.context;
		void tick().then(() => {
			if (this.cluster?.context !== context) return;
			if (filter) this.filterRequest = filter;
			if (cursor) this.cursorRequest = cursor;
		});
	}

	/** Opens the view `context` showed when Kubri last ran there, if the cluster still has its kinds. */
	private openSaved(context: string) {
		const view = savedView(context);
		if (!view || !(PAGES as readonly string[]).includes(view.page)) return;
		const resource = view.resource && findResource(this.resources, view.resource);
		if (view.resource && !resource) return;
		const xray = view.xray && findResource(this.resources, view.xray);
		if (view.page === 'xray' && !xray) return;
		this.page = view.page as Page;
		if (resource) this.selected = resource;
		if (xray) this.xrayResource = xray;
		this.namespace = view.namespace;
	}

	/**
	 * Opens the view the address named, if it is in `context` and the cluster has its kinds;
	 * false when there is no such address.
	 */
	private openAddressed(context: string): boolean {
		const address = this.#addressed;
		if (!address) return false;
		if (address.context !== context) {
			// Switching here would switch everyone who uses this server.
			this.toast(t('This address is for {context}; switch to that context to open it', { context: address.context }));
			return false;
		}
		const resource = address.resource && findResource(this.resources, address.resource);
		if (address.resource && !resource) return false;
		const xray = address.xray && findResource(this.resources, address.xray);
		if (address.page === 'xray' && !xray) return false;
		this.page = address.page;
		if (resource) this.selected = resource;
		if (xray) this.xrayResource = xray;
		this.namespace = address.namespace;
		this.scope = address.scope;
		// The address wins over the k9s `defaultView`, as the view was open before a reload.
		this.#startup = false;
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
						this.toast(t('{context} has no {kind}', { context: action.context, kind: action.resource.plural }), 'bad');
						return;
					}
				}
				this.open(resource, action.namespace, action.filter);
				break;
			}
			case 'context': {
				const name = action.name === '-' ? this.previousContext : action.name;
				if (!name) this.toast(t('There is no previous context yet'), 'bad');
				else if (name !== this.cluster?.context) await this.switchContext(name);
				break;
			}
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
			case 'new':
				this.showCreate(action.resource, action.params);
				break;
			case 'sanitizer':
				this.showSanitizer(action.namespace);
				break;
			case 'benchmarks':
				this.showBenchmarks();
				break;
			case 'integrations':
				this.showIntegrations();
				break;
			case 'rightsizing':
				this.showRightSizing();
				break;
			case 'tools':
				this.showTools();
				break;
			case 'kinds':
				this.showKinds(action.filter);
				break;
			case 'fleet':
				this.showFleet();
				break;
			case 'terminal':
				this.openLocalTerminal(action.another);
				break;
			case 'search':
				this.showSearch(action.query);
				break;
			case 'graph':
				this.showGraph(action.namespace);
				break;
			case 'changes':
				this.showChanges(action.filter);
				break;
			case 'snapshot':
				this.showSnapshot(action.namespace);
				break;
			case 'kubeconfig':
				this.kubeconfigFilesOpen = true;
				break;
			case 'object':
				await this.openObject(action.target, action.kind);
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
			case 'feedback':
				backend.openUrl(feedbackLink(action.kind, backend.desktop)).catch((err) => this.toast(errorMessage(err), 'bad'));
				break;
			case 'call':
				action.run();
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
		if (this.cluster) {
			this.recent = remember({
				context: this.cluster.context,
				kind: resource.kind,
				...refOf(resource),
				namespace: target.namespace,
				name: target.name
			});
		}
	}

	/** Objects opened lately, newest first, in any context. */
	recent = $state.raw<RecentObject[]>(recentObjects());

	/** Kinds and objects kept in the sidebar, for every context; the sidebar shows the current one's. */
	pins = $state.raw<Pin[]>(loadPins());

	/** The pins of the connected context, in the order they were made. */
	get pinsHere(): Pin[] {
		const context = this.cluster?.context;
		return context ? this.pins.filter((p) => p.context === context) : [];
	}

	pinFor(resource: ResourceInfo, target?: { namespace: string | null; name: string }): Pin | null {
		const context = this.cluster?.context;
		if (!context) return null;
		return {
			context,
			kind: resource.kind,
			...refOf(resource),
			namespace: target?.namespace ?? null,
			name: target?.name ?? ''
		};
	}

	isPinned(pin: Pin | null): boolean {
		return !!pin && this.pins.some((p) => samePin(p, pin));
	}

	togglePin(pin: Pin | null) {
		if (!pin) return;
		const was = this.isPinned(pin);
		this.pins = togglePinIn(this.pins, pin);
		savePins(this.pins);
		this.toast(was ? t('Unpinned {name}', { name: pin.name || pin.kind }) : t('Pinned {name} to the sidebar', { name: pin.name || pin.kind }));
	}

	/** Opens a pin or a recent object: the list of a kind, a namespace's pods, or an object in its list. */
	openPin(pin: RecentObject) {
		const resource = findResource(this.resources, { group: pin.group, version: pin.version, plural: pin.plural });
		if (!resource) {
			this.toast(t('The cluster does not serve {kind}', { kind: pin.plural }), 'bad');
			return;
		}
		if (!pin.name) {
			this.open(resource);
		} else if (resource.plural === 'namespaces' && resource.group === '') {
			const pods = findResource(this.resources, { group: '', version: 'v1', plural: 'pods' });
			if (pods) this.open(pods, pin.name, '');
		} else {
			this.openAt(resource, pin.namespace, pin.name);
		}
	}

	/**
	 * What the palette offers for the row under the cursor: the open list sets it when it appears
	 * and clears it when it goes. Only a function, so nothing is worked out until the palette asks.
	 */
	rowActions: (() => { label: string; actions: MenuAction[] } | null) | null = null;

	/** Opens an object's details, connecting to its context first if it is another one. */
	async openObject(target: ObjectRef, kind = target.resource.plural) {
		if (target.context && target.context !== this.cluster?.context) {
			await this.switchContext(target.context);
			if (this.cluster?.context !== target.context) return;
		}
		const resource = findResource(this.resources, target.resource);
		if (!resource) {
			this.toast(this.cluster ? t('{context} has no {kind}', { context: this.cluster.context, kind }) : t('This cluster has no {kind}', { kind }), 'bad');
			return;
		}
		this.showDetails(resource, { resource: refOf(resource), namespace: target.namespace, name: target.name });
	}

	/** Searches objects by name, label and field, here or in several contexts. */
	showSearch(query?: string) {
		if (drafts.block(() => this.showSearch(query))) return;
		this.page = 'search';
		this.details = null;
		if (query !== undefined) this.searchQuery = query;
	}

	/** What changed: the changes Kubri recorded, for the namespace or one object. */
	showChanges(filter: ChangesFilter = {}) {
		if (drafts.block(() => this.showChanges(filter))) return;
		this.page = 'changes';
		this.details = null;
		this.changesFilter = { namespace: this.namespace || undefined, ...filter };
	}

	/** What the cluster runs that Kubri uses: Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager. */
	showIntegrations() {
		if (drafts.block(() => this.showIntegrations())) return;
		this.page = 'integrations';
		this.details = null;
	}

	/** Every context at once, without connecting to them. */
	showFleet() {
		if (drafts.block(() => this.showFleet())) return;
		this.page = 'fleet';
		this.details = null;
	}

	/** Every tool beyond the lists, on one page. */
	showTools() {
		if (drafts.block(() => this.showTools())) return;
		this.page = 'tools';
		this.details = null;
	}

	/** Opens a tool of the sidebar or the Tools page. */
	showTool(id: ToolId) {
		switch (id) {
			case 'xray': {
				const resource = this.xrayResource ?? xrayResources(this.resources)[0];
				if (resource) this.showXray(resource);
				break;
			}
			case 'changes':
				return this.showChanges();
			case 'helm':
				return this.showHelm();
			case 'portforwards':
				return this.showPortForwards();
			case 'rbac':
				return this.showRbac();
			case 'sanitizer':
				return this.showSanitizer();
			case 'rightsizing':
				return this.showRightSizing();
			case 'benchmarks':
				return this.showBenchmarks();
		}
	}

	/** Every kind the cluster serves, to find one by name. */
	showKinds(filter = '') {
		if (drafts.block(() => this.showKinds(filter))) return;
		this.page = 'kinds';
		this.details = null;
		this.kindsFilter = filter;
	}

	/** Containers whose requests or limits are far from what they use. */
	showRightSizing() {
		if (drafts.block(() => this.showRightSizing())) return;
		this.page = 'rightsizing';
		this.details = null;
	}

	/** The Overview, optionally switching namespace like `:pulses kube-system`. */
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

	/** XRay as one graph of how the objects connect, with the request paths of Ingresses. */
	showGraph(namespace?: string) {
		if (drafts.block(() => this.showGraph(namespace))) return;
		this.page = 'graph';
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

	/** The Create page; with `resource`, starting from its template, like `:new deploy web --image nginx`. */
	showCreate(resource?: ResourceInfo, params: TemplateParams = {}) {
		if (drafts.block(() => this.showCreate(resource, params))) return;
		this.page = 'create';
		this.details = null;
		if (resource) {
			const namespace = params.namespace ?? (resource.namespaced && this.namespace ? this.namespace : undefined);
			creating.start(resource, { ...params, namespace });
		}
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
			this.toast(t('Could not read the k9s configuration: {error}', { error: errorMessage(err) }), 'bad');
		} finally {
			this.starting = false;
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
				if (!theme) error = t('The skin {name} sets no colors Kubri can use.', { name });
			} catch (err) {
				error = t('Could not load the skin {name}: {error}', { name, error: errorMessage(err) });
			}
		}
		// A later choice may have finished first.
		if (request !== this.#skinRequest) return;
		applyTheme(theme);
		this.skin = theme ? name : null;
		this.skinDark = theme?.scheme === 'dark';
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

	/** Remembers where the current context is, to return there after switching away. */
	private savePlace() {
		const cluster = this.cluster;
		if (!cluster || cluster.snapshot) return;
		const focus = this.focus?.();
		const target = focus?.target;
		this.#places.save(cluster.context, {
			view: this.view,
			history: this.#history,
			historyAt: this.#historyAt,
			filter: focus?.filter ?? '',
			cursor: target ? { namespace: target.namespace ?? null, name: target.name } : null
		});
	}

	/** Asks which contexts the core keeps connected, after a switch changed them. */
	private async refreshWarm() {
		try {
			this.warm = (await backend.contexts()).warm ?? [];
		} catch {
			// The switcher only shows fewer marks.
		}
	}

	/** Asks the web server once whether it allows local terminals. */
	private askLocalTerminal() {
		if (this.#localTerminalAsked || backend.desktop) return;
		this.#localTerminalAsked = true;
		backend.localTerminalInfo().then(
			(info) => (this.localTerminal = info.enabled),
			() => (this.#localTerminalAsked = false)
		);
	}

	/**
	 * Shows the local terminal of this context, or opens one with kubectl and helm set to it and
	 * the namespace shown; `another` opens a new one also when one is open.
	 */
	openLocalTerminal(another = false) {
		const cluster = this.cluster;
		if (!cluster || cluster.snapshot) {
			this.toast(cluster ? t('A snapshot has no cluster for kubectl to talk to') : t('Connect to a context to open a terminal'), 'bad');
			return;
		}
		if (!this.localTerminal) {
			this.toast(t('The local terminal is off here, as it would run on the server. Start the server with KUBRI_LOCAL_TERMINAL=true to allow it.'), 'bad');
			return;
		}
		dock.local(cluster.context, this.namespace === ALL_NAMESPACES ? null : this.namespace, another);
	}

	private async connected(cluster: ClusterInfo) {
		const left = this.cluster;
		const before = this.view;
		if (left && !left.snapshot && left.context !== cluster.context) this.previousContext = left.context;
		this.details = null;
		this.resources = [];
		this.cluster = null;
		const resources = await backend.resources();
		const previous = this.selected;
		this.cluster = cluster;
		this.k9s = null;
		access.reset(cluster.context);
		this.#guardRequest++;
		this.applyGuard({ readOnly: true, reason: t('Kubri is still checking whether it may change this cluster'), canUnlock: false, unlockedFor: null });
		this.refreshGuard();
		this.askLocalTerminal();
		this.resources = resources;
		const closed = dock.keepContexts([...this.contexts.map((c) => c.name), cluster.context]);
		if (closed) this.toast(plural(closed, 'Closed {n} workspace tab of a context that is gone', 'Closed {n} workspace tabs of a context that is gone'));
		this.namespace = cluster.snapshot
			? (cluster.snapshot.namespace ?? ALL_NAMESPACES)
			: this.contextSettingsOf(cluster.context).namespace || ALL_NAMESPACES;
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
		const place = cluster.snapshot ? undefined : this.#places.take(cluster.context);
		if (place) this.returnTo(place, before);
		else if (!cluster.snapshot && !this.openAddressed(cluster.context)) this.openSaved(cluster.context);
		if (!cluster.snapshot) {
			this.startFallback = null;
			rememberContext(cluster.context, this.contexts.find((c) => c.isDefault)?.name ?? null);
		}
		this.#addressed = null;
		// Hotkeys, plugins and skins can differ per context.
		this.loadK9s();
		this.startFavoriteForwards();
		this.integrations = null;
		this.loadIntegrations(false);
		this.loadViews();
		if (cluster.resumed) this.refreshResources(cluster.context);
	}

	/**
	 * A switch to a kept connection uses the resource types found when it was made; this asks
	 * the cluster again and updates the sidebar when they changed, such as after a CRD install.
	 */
	private async refreshResources(context: string) {
		try {
			const fresh = await backend.refreshResources();
			if (this.cluster?.context !== context) return;
			const same = fresh.length === this.resources.length && fresh.every((r, i) => sameResource(r, this.resources[i]) && r.kind === this.resources[i].kind);
			if (same) return;
			this.resources = fresh;
			const selected = this.selected;
			this.selected = fresh.find((r) => selected && sameResource(r, selected)) ?? fresh.find((r) => r.group === '' && r.plural === 'pods') ?? null;
		} catch {
			// The kept list stays; the next switch or reload asks again.
		}
	}

	/** Reads the view files again, so an edited one shows on the next connect or reload. */
	async loadViews() {
		try {
			this.views = await backend.views();
		} catch {
			// Without them, custom resources show the server's columns.
		}
	}

	/** The view of a kind, if one is loaded. */
	viewOf(resource: Pick<ResourceInfo, 'group' | 'kind'>): KindView | undefined {
		return this.views?.views.find((v) => v.group === resource.group && v.kind === resource.kind);
	}

	/** Finds the cluster's tools; `fresh` looks again and asks each whether it answers. */
	async loadIntegrations(fresh: boolean) {
		const context = this.cluster?.context;
		try {
			const found = await backend.integrations(fresh);
			if (this.cluster?.context === context) this.integrations = found;
		} catch {
			// Without them, Kubri works as before.
		}
	}

	/** Whether the cluster runs `id` and Kubri may use it. */
	has(id: IntegrationId): boolean {
		const item = this.integrations?.items.find((i) => i.id === id);
		return !!item && (item.source === 'found' || item.source === 'settings');
	}

	/** Starts the connected context's favorite port-forwards, as after connecting. */
	async startFavoriteForwards() {
		let results: FavoriteStart[];
		try {
			results = await backend.startFavoriteForwards();
		} catch {
			return;
		}
		if (!results.length) return;
		this.refreshForwards();
		const failed = results.filter((r) => r.error);
		const started = results.length - failed.length;
		if (started) this.toast(plural(started, 'Started {n} favorite port-forward', 'Started {n} favorite port-forwards'));
		for (const { favorite, error } of failed) {
			this.toast(t('{name} did not start: {error}', { name: favoriteLabel(favorite), error: coreText(error ?? '') }), 'bad');
		}
	}

	/** The favorite a running forward is, if it is one. */
	favoriteOf(forward: PortForwardInfo): FavoriteForward | undefined {
		return this.contextSettingsOf(forward.context).forwards?.find((f) => isForwardOf(f, forward));
	}

	/** The connected context's favorite port-forwards. */
	get favoritesHere(): FavoriteForward[] {
		return this.cluster ? (this.contextSettingsOf(this.cluster.context).forwards ?? []) : [];
	}

	/** The running forward of a favorite of the connected context, if it runs. */
	runningFavorite(favorite: FavoriteForward): PortForwardInfo | undefined {
		return this.forwards.find((f) => f.context === this.cluster?.context && isForwardOf(favorite, f));
	}

	/** Favorites being started or stopped, so a second click waits for the first. */
	switchingFavorites = $state.raw<FavoriteForward[]>([]);

	/** Starts a favorite of the connected context, or stops it if it runs. */
	async switchFavorite(favorite: FavoriteForward) {
		const context = this.cluster?.context;
		if (!context || this.switchingFavorites.includes(favorite)) return;
		this.switchingFavorites = [...this.switchingFavorites, favorite];
		let running: PortForwardInfo | undefined;
		try {
			await this.refreshForwards();
			running = this.runningFavorite(favorite);
			if (running) {
				await backend.stopPortForward(running.id);
				this.toast(t('Stopped {name}', { name: favoriteLabel(favorite) }));
			} else {
				const f = await backend.startPortForward({
					target: { ...favorite.target, context },
					remotePort: favorite.remotePort,
					localPort: favorite.localPort,
					address: favorite.address || null
				});
				this.toast(t('Forwarding {local} to {pod}', { local: `${f.address}:${f.localPort}`, pod: `${f.pod}:${f.podPort}` }));
			}
		} catch (err) {
			this.toast(running ? errorMessage(err) : t('{name} did not start: {error}', { name: favoriteLabel(favorite), error: errorMessage(err) }), 'bad');
		} finally {
			this.switchingFavorites = this.switchingFavorites.filter((f) => f !== favorite);
			await this.refreshForwards();
		}
	}

	/** Whether a favorite of the connected context starts whenever Kubri connects. */
	async setStartOnConnect(favorite: FavoriteForward, startOnConnect: boolean) {
		const context = this.cluster?.context;
		if (!context) return;
		const settings = this.contextSettingsOf(context);
		const forwards = (settings.forwards ?? []).map((f) => (f === favorite ? { ...f, startOnConnect } : f));
		await this.setContextSettings(context, { ...settings, forwards });
	}

	/** Keeps a running forward as a favorite of its context, or forgets it. */
	async toggleFavorite(forward: PortForwardInfo) {
		const settings = this.contextSettingsOf(forward.context);
		const kept = settings.forwards ?? [];
		const known = this.favoriteOf(forward);
		const { context: _, ...target } = forward.target;
		const forwards = known
			? kept.filter((f) => f !== known)
			: [
					...kept,
					{
						target,
						remotePort: forward.remotePort,
						localPort: forward.localPort,
						address: forward.address === '127.0.0.1' ? '' : forward.address,
						startOnConnect: false
					}
				];
		if (await this.setContextSettings(forward.context, { ...settings, forwards })) {
			this.toast(
			known
				? t('{name} is no longer a favorite', { name: favoriteLabel(known) })
				: t('{name} is a favorite: start it with one click under Port-forwards in the sidebar', { name: favoriteLabel(forwards.at(-1)!) })
		);
		}
	}

	/** Forgets a favorite of the connected context. */
	async removeFavorite(favorite: FavoriteForward) {
		const context = this.cluster?.context;
		if (!context) return;
		const settings = this.contextSettingsOf(context);
		await this.setContextSettings(context, { ...settings, forwards: (settings.forwards ?? []).filter((f) => f !== favorite) });
	}
}

/** Whether a running forward is a favorite. */
export function isForwardOf(favorite: FavoriteForward, forward: PortForwardInfo) {
	return (
		sameResource(favorite.target.resource, forward.target.resource) &&
		(favorite.target.namespace ?? null) === (forward.target.namespace ?? null) &&
		favorite.target.name === forward.target.name &&
		favorite.remotePort === forward.remotePort &&
		favorite.localPort === forward.localPort
	);
}

export function favoriteLabel(f: FavoriteForward) {
	return t('{target} on port {port}', { target: `${f.target.resource.plural}/${f.target.name}:${f.remotePort}`, port: f.localPort });
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

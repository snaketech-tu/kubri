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
	type ClusterInfo,
	type BenchInfo,
	type ContextInfo,
	type Drill,
	type ExplainSubject,
	type K9sConfig,
	type ObjectRef,
	type Plugin,
	type PluginRun,
	type PortForwardInfo,
	type ResourceInfo
} from './api';
import { suggest, type Action, type ConfigSection, type PaletteConfig } from './commands';
import { errorMessage } from './format';
import { plural, t } from './i18n';
import { coreText } from './i18n/errors';
import { Places, type Place } from './places';
import { fromHash, toHash, type Address } from './address';
import { keepViews, lastContext, rememberContext, saveView, savedView, startContext, startSetting } from './startup';
import type { Focus, PluginView } from './k9s';
import type { Menu, MenuAction } from './menu';
import { xrayResources } from './xray';
import { productionRule, withMarked, type ProductionRule } from './production';
import type { ToolId } from './nav';
import { Activities } from './state/activity.svelte';
import { Appearance } from './state/appearance.svelte';
import { FavoriteForwards } from './state/favorites.svelte';
import { GuardRails } from './state/guard.svelte';
import { History } from './state/history.svelte';
import { ClusterIntegrations } from './state/integrations.svelte';
import { Layout } from './state/layout.svelte';
import {
	ALL_NAMESPACES,
	PAGES,
	findResource,
	refOf,
	sameObject,
	sameResource,
	sameView,
	type Details,
	type DetailsExtra,
	type DetailsTab,
	type Dialog,
	type ListScope,
	type Page,
	type Toast,
	type View
} from './state/model';
import { Pins } from './state/pins.svelte';
import { Releases } from './state/releases.svelte';
import { run as runAction } from './state/run';
import { Preferences } from './state/settings.svelte';
import { KindViews } from './state/views.svelte';

export * from './state/model';
export { NO_CONTEXT_SETTINGS, legacyContextSettings } from './state/settings.svelte';
export { favoriteLabel, isForwardOf } from './state/favorites.svelte';

/**
 * Global app state: kubeconfig contexts, the connected cluster and what is being browsed. The
 * settings, guard rails, appearance, layout, integrations, kind views, pins, favorite
 * port-forwards, activity and releases have stores of their own in `state/`.
 */
export class AppState {
	/** Kubri's settings file: names, colors and guard rails per context, and preferences. */
	prefs = new Preferences(this);
	/** Whether the core lets Kubri change the connected cluster. */
	rails = new GuardRails(this);
	pins = new Pins(this);
	favorites = new FavoriteForwards(this);
	activity = new Activities(this);
	releases = new Releases(this);
	appearance = new Appearance(this);
	layout = new Layout();
	integrations = new ClusterIntegrations(this);
	views = new KindViews();

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
	/** The details panel takes the place of the list: maximized, or showing logs or a shell. */
	get panelWide(): boolean {
		const tab = this.#details?.tab;
		return !!this.#details && (this.layout.maximized || ((tab === 'logs' || tab === 'shell') && !this.layout.toolsBeside));
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
	#history = new History<View>(sameView);
	/** Set when going back or forth, so the view it brings up is not recorded again. */
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
	/** The settings of the connected context. */
	contextSettings = $derived(this.prefs.contextSettingsOf(this.cluster?.context));
	/** A label such as "Production" for this context, set by the user; Kubri never guesses it from names. */
	environment = $derived(this.contextSettings.label);
	/** Marked as production by the user: a red stripe and badge, and typed names before destructive actions. */
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
		const settings = this.prefs.contextSettingsOf(context);
		if (await this.prefs.setContextSettings(context, { ...settings, productionNamespaces: withMarked(settings, names, production) })) {
			const name = names.join(', ');
			const n = names.length;
			// Unmarked by name, but a pattern or label rule still marks it.
			const still = production ? null : names.map((n) => this.productionRuleOf(n)).find(Boolean);
			if (still) this.toast(t('Production by the rule {rule}. Change it in the context settings.', { rule: still.rule }));
			else if (n > 1) {
				this.toast(production ? plural(n, '{n} namespace is now production', '{n} namespaces are now production') : plural(n, '{n} namespace is no longer production', '{n} namespaces are no longer production'));
			} else this.toast(production ? t('{name} is now a production namespace', { name }) : t('{name} is no longer a production namespace', { name }));
		}
	}
	/** k9s `readOnly`, read-only or locked in Kubri: nothing that changes the cluster is offered. */
	readOnly = $derived((this.k9s?.readOnly ?? false) || this.rails.guard.readOnly);

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
		this.releases.check(true);
		try {
			await this.prefs.load();
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
				? t('Stopped connecting to {name}; still on {current}', { name, current: this.prefs.contextName(this.cluster.context) })
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
		this.#history.push(view);
	}

	/** Goes back (`[`) or forward (`]`) in the history; false when there is nowhere to go. */
	get canGoBack() {
		return this.#history.canGoBack;
	}

	get canGoForward() {
		return this.#history.canGoForward;
	}

	go(delta: -1 | 1): boolean {
		if (drafts.block(() => this.go(delta))) return false;
		const view = this.#history.step(delta);
		if (!view) return false;
		this.restore(view);
		return true;
	}

	/** Returns to the view before this one, and back again (`-`). */
	toggleLast(): boolean {
		if (drafts.block(() => this.toggleLast())) return false;
		const last = this.#history.toggleLast();
		if (!last) return false;
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
		this.#history.reset(history, historyAt);
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
	run(action: Action) {
		return runAction(this, action);
	}

	/** Opens `page`, and closes the details; false while an unsaved YAML draft holds it until `again` runs. */
	#show(page: Page, again: () => void, namespace?: string): boolean {
		if (drafts.block(again)) return false;
		this.page = page;
		this.details = null;
		this.setNamespace(namespace);
		return true;
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
		this.pins.opened(resource, target);
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
		if (!this.#show('search', () => this.showSearch(query))) return;
		if (query !== undefined) this.searchQuery = query;
	}

	/** What changed: the changes Kubri recorded, for the namespace or one object. */
	showChanges(filter: ChangesFilter = {}) {
		if (!this.#show('changes', () => this.showChanges(filter))) return;
		this.changesFilter = { namespace: this.namespace || undefined, ...filter };
	}

	/** What the cluster runs that Kubri uses: Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager. */
	showIntegrations() {
		this.#show('integrations', () => this.showIntegrations());
	}

	/** Every context at once, without connecting to them. */
	showFleet() {
		this.#show('fleet', () => this.showFleet());
	}

	/** Every tool beyond the lists, on one page. */
	showTools() {
		this.#show('tools', () => this.showTools());
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
		if (!this.#show('kinds', () => this.showKinds(filter))) return;
		this.kindsFilter = filter;
	}

	/** Containers whose requests or limits are far from what they use. */
	showRightSizing() {
		this.#show('rightsizing', () => this.showRightSizing());
	}

	/** The Overview, optionally switching namespace like `:pulses kube-system`. */
	showPulses(namespace?: string) {
		this.#show('pulses', () => this.showPulses(namespace), namespace);
	}

	/** The XRay trees of `resource`, like k9s `:xray deploy`. */
	showXray(resource: ResourceInfo, namespace?: string) {
		if (!this.#show('xray', () => this.showXray(resource, namespace), namespace)) return;
		this.xrayResource = resource;
	}

	/** XRay as one graph of how the objects connect, with the request paths of Ingresses. */
	showGraph(namespace?: string) {
		this.#show('graph', () => this.showGraph(namespace), namespace);
	}

	/** Who may do what, like k9s `:rbac`. */
	showRbac(namespace?: string) {
		this.#show('rbac', () => this.showRbac(namespace), namespace);
	}

	/** The Create page; with `resource`, starting from its template, like `:new deploy web --image nginx`. */
	showCreate(resource?: ResourceInfo, params: TemplateParams = {}) {
		if (!this.#show('create', () => this.showCreate(resource, params))) return;
		if (resource) {
			const namespace = params.namespace ?? (resource.namespaced && this.namespace ? this.namespace : undefined);
			creating.start(resource, { ...params, namespace });
		}
	}

	/** Helm releases, like k9s `:helm`. */
	showHelm(namespace?: string) {
		this.#show('helm', () => this.showHelm(namespace), namespace);
	}

	/** Problems and risky settings, like Popeye; `:popeye kube-system` switches namespace. */
	showSanitizer(namespace?: string) {
		this.#show('sanitizer', () => this.showSanitizer(namespace), namespace);
	}

	/** HTTP benchmarks, optionally showing one of them. */
	showBenchmarks(id?: number) {
		if (!this.#show('benchmarks', () => this.showBenchmarks(id))) return;
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
		if (!this.#show('config', () => this.showConfig(section))) return;
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
			this.rails.refresh();
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
		await this.appearance.loadSkin(context);
	}

	showPortForwards() {
		if (!this.#show('portforwards', () => this.showPortForwards())) return;
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
			history: this.#history.entries,
			historyAt: this.#history.at,
			filter: focus?.filter ?? '',
			cursor: target ? { namespace: target.namespace ?? null, name: target.name } : null
		});
	}

	/** Asks which contexts the core keeps connected, after a switch changed them. */
	async refreshWarm() {
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
		this.rails.checking();
		this.askLocalTerminal();
		this.resources = resources;
		const closed = dock.keepContexts([...this.contexts.map((c) => c.name), cluster.context]);
		if (closed) this.toast(plural(closed, 'Closed {n} workspace tab of a context that is gone', 'Closed {n} workspace tabs of a context that is gone'));
		this.namespace = cluster.snapshot
			? (cluster.snapshot.namespace ?? ALL_NAMESPACES)
			: this.prefs.contextSettingsOf(cluster.context).namespace || ALL_NAMESPACES;
		this.scope = null;
		// Views of the last cluster may not exist in this one.
		this.#history.reset();
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
		this.favorites.startAll();
		this.integrations.found = null;
		this.integrations.load(false);
		this.views.load();
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
}

export const app = new AppState();

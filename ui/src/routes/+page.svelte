<script lang="ts">
	import { drafts } from '#lib/navigation.svelte.js';
	import { applyScheme } from '#lib/skin.js';
	import Icon from '#lib/components/Icon.svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { untrack } from 'svelte';
	import { forControl } from '#lib/keys.js';
	import logo from '#lib/assets/logo.svg';
	import { canDelete, canEdit, canRestart, canScale } from '#lib/actions.js';
	import { listenForBindings } from '#lib/bindings.js';
	import ActionDialog from '#lib/components/ActionDialog.svelte';
	import ActionMenu from '#lib/components/ActionMenu.svelte';
	import ExplainPanel from '#lib/components/ExplainPanel.svelte';
	import BenchmarksView from '#lib/components/BenchmarksView.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import ConfigView from '#lib/components/ConfigView.svelte';
	import ContextSettingsForm from '#lib/components/ContextSettingsForm.svelte';
	import ContextSwitcher from '#lib/components/ContextSwitcher.svelte';
	import CreateView from '#lib/components/CreateView.svelte';
	import DetailsPanel from '#lib/components/DetailsPanel.svelte';
	import Dock from '#lib/components/Dock.svelte';
	import DebugDialog from '#lib/components/DebugDialog.svelte';
	import CompareDialog from '#lib/components/CompareDialog.svelte';
	import ForbiddenDialog from '#lib/components/ForbiddenDialog.svelte';
	import AgentRequests from '#lib/components/AgentRequests.svelte';
	import HelmView from '#lib/components/HelmView.svelte';
	import KubectlDialog from '#lib/components/KubectlDialog.svelte';
	import HelpOverlay from '#lib/components/HelpOverlay.svelte';
	import NamespacePicker from '#lib/components/NamespacePicker.svelte';
	import PluginTerminal from '#lib/components/PluginTerminal.svelte';
	import PortForwardsView from '#lib/components/PortForwardsView.svelte';
	import PulsesView from '#lib/components/PulsesView.svelte';
	import RbacView from '#lib/components/RbacView.svelte';
	import ResourceView from '#lib/components/ResourceView.svelte';
	import SanitizerView from '#lib/components/SanitizerView.svelte';
	import IntegrationsView from '#lib/components/IntegrationsView.svelte';
	import RightSizingView from '#lib/components/RightSizingView.svelte';
	import FleetView from '#lib/components/FleetView.svelte';
	import SearchView from '#lib/components/SearchView.svelte';
	import GraphView from '#lib/components/GraphView.svelte';
	import ChangesView from '#lib/components/ChangesView.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import XrayView from '#lib/components/XrayView.svelte';
	import { parseShortcut, pluginApplies, shortcutLabel } from '#lib/k9s.js';
	import { keyFromLabel } from '#lib/menu.js';
	import {
		canPortForward,
		hasImages,
		hasLogs,
		hasUsers,
		isCronJobs,
		isNamespaces,
		isNodes,
		isPods,
		isReplicaSets,
		isSecrets
	} from '#lib/pods.js';
	import { backend } from '#lib/api/index.js';
	import { dock } from '#lib/dock.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { app } from '#lib/state.svelte.js';
	import { LiveTable } from '#lib/watch.svelte.js';

	const namespaces = new LiveTable();

	$effect(() => {
		// Also after signing in again, when a message from before may still be up.
		app.error = null;
		app.load();
	});

	$effect(() => untrack(() => applyScheme(app.scheme)));

	// k9s plugin and hotkey keys.
	$effect(() => listenForBindings());

	$effect(() => {
		if (!app.cluster) return;
		return namespaces.start({ group: '', version: 'v1', plural: 'namespaces' }, null);
	});

	/** The context's settings name the namespaces to show, by list or by label. */
	const limited = $derived(app.contextSettings.namespaces.length > 0 || !!app.contextSettings.namespaceSelector.trim());

	$effect(() => {
		if (!limited) app.namespaces = namespaces.rows.map((r) => r.name).sort();
	});

	// Someone who may not list namespaces still gets the ones the kubeconfig and settings name.
	$effect(() => {
		if (!app.cluster || (!limited && (!namespaces.error || namespaces.rows.length))) return;
		// Read so that changing the limit asks again.
		void [app.contextSettings.namespaces, app.contextSettings.namespaceSelector];
		const context = app.cluster.context;
		backend.readableNamespaces().then(
			(names) => {
				if (app.cluster?.context === context && (limited || !namespaces.rows.length)) app.namespaces = names;
			},
			() => {}
		);
	});

	// Every place Kubri shows goes into the history, for `[`, `]` and `-`.
	$effect(() => {
		const view = app.view;
		untrack(() => app.remember(view));
	});

	// Port-forwards and benchmarks outlive context switches; keep the sidebar counts current.
	$effect(() => {
		app.refreshForwards();
		app.refreshBenchmarks();
		const timer = setInterval(() => {
			app.refreshForwards();
			app.refreshBenchmarks();
		}, 5000);
		return () => clearInterval(timer);
	});

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		const typing = forControl(e);
		if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
			e.preventDefault();
			app.paletteOpen = true;
		} else if (!typing && e.key === ':') {
			e.preventDefault();
			app.paletteOpen = true;
		} else if (!typing && e.key === '?') {
			e.preventDefault();
			app.helpOpen = true;
		} else if (!typing && !e.ctrlKey && !e.metaKey && (e.key === '[' || e.key === ']' || e.key === '-')) {
			// Back, forward and the last view, as in k9s.
			e.preventDefault();
			if (e.key === '-') app.toggleLast();
			else app.go(e.key === '[' ? -1 : 1);
		} else if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
			e.preventDefault();
			app.go(e.key === 'ArrowLeft' ? -1 : 1);
		}
	}

	// The back and forward buttons of a mouse.
	function onmouseup(e: MouseEvent) {
		if ((e.button !== 3 && e.button !== 4) || app.overlayOpen) return;
		e.preventDefault();
		app.go(e.button === 3 ? -1 : 1);
	}

	const viewHints = $derived.by(() => {
		const panel: [string, string][] = app.details ? [['f', app.maximized ? 'restore' : 'maximize'], ['esc', 'close']] : [];
		// The log keys, while the logs are on screen.
		if (app.details?.tab === 'logs') {
			return [
				['/', 'search'],
				['n/N', 'match'],
				['0–6', 'range'],
				['t', 'timestamps'],
				['w', 'wrap'],
				['s', 'autoscroll'],
				['m', 'mark'],
				['x', 'explain'],
				['c', 'copy'],
				['^s', 'save'],
				...panel,
				['?', 'help']
			] as [string, string][];
		}
		if (app.page === 'portforwards') {
			return [
				[':', 'command'],
				['c', 'copy url'],
				['o', 'open'],
				['s', 'favorite'],
				['b', 'benchmark'],
				['^d', 'stop'],
				['?', 'help']
			] as [string, string][];
		}
		if (app.page === 'pulses') {
			return [[':', 'command'], ['←→', 'move'], ['enter', 'open list'], ...panel, ['?', 'help']] as [string, string][];
		}
		if (app.page === 'xray') {
			return [
				[':', 'command'],
				['/', 'filter'],
				['h/l', 'collapse/expand'],
				['space', 'toggle'],
				['enter', 'describe'],
				...panel,
				['?', 'help']
			] as [string, string][];
		}
		if (app.page === 'rbac') {
			return [[':', 'command'], ['/', 'filter'], ['j/k', 'move'], ...panel, ['?', 'help']] as [string, string][];
		}
		if (app.page === 'sanitizer') {
			return [
				[':', 'command'],
				['/', 'filter'],
				['h/l', 'collapse/expand'],
				['enter', 'describe'],
				['⇧e', 'explain'],
				['⇧r', 'scan again'],
				...panel,
				['?', 'help']
			] as [string, string][];
		}
		if (app.page === 'create') {
			return [[':', 'command'], ['^s', 'review'], ['esc', 'back to editing'], ['?', 'help']] as [string, string][];
		}
		if (app.page === 'config') {
			return [[':', 'command'], ['⇧r', 'reload'], ['?', 'help']] as [string, string][];
		}
		if (app.page === 'benchmarks') {
			return [[':', 'command'], ['j/k', 'move'], ['b', 'run again'], ['^d', 'stop/remove'], ['?', 'help']] as [string, string][];
		}
		if (app.page === 'helm') {
			return [
				[':', 'command'],
				['/', 'filter'],
				['enter', 'open'],
				['v', 'values'],
				['h', 'history'],
				['c', 'compare'],
				['x', 'drift'],
				['u', 'upgrade'],
				['i', 'install'],
				['r', 'roll back'],
				['^d', 'uninstall'],
				...panel,
				['?', 'help']
			] as [string, string][];
		}
		const r = app.selected;
		const h: [string, string][] = [
			[':', 'command'],
			['/', 'filter'],
			['enter', 'open'],
			['d', 'describe'],
			['y', 'yaml']
		];
		if (!r) return h;
		const writable = !app.readOnly;
		if (hasLogs(r)) h.push(['l', 'logs']);
		if (isPods(r) && writable) h.push(['s', 'shell'], ['a', 'attach']);
		if (isPods(r)) h.push(['o', 'node']);
		if (isSecrets(r)) h.push(['x', 'decode']);
		if (isNamespaces(r)) h.push(['u', 'use']);
		if (hasUsers(r)) h.push(['u', 'used by']);
		if (canPortForward(r)) h.push(['⇧f', 'port-forward']);
		if (isNodes(r) && writable) h.push(['c', 'cordon'], ['u', 'uncordon'], ['r', 'drain']);
		if (isCronJobs(r) && writable) h.push(['t', 'trigger'], ['s', 'suspend']);
		if (hasImages(r) && writable) h.push(['i', 'image']);
		if (isReplicaSets(r) && writable) h.push(['^l', 'roll back']);
		if (canEdit(r) && writable) h.push(['e', 'edit']);
		if (canDelete(r) && writable) h.push(['^d', 'delete']);
		if (canScale(r) && !isPods(r) && writable) h.push(['s', 'scale']);
		if (canRestart(r) && writable) h.push(['r', 'restart']);
		if (app.scope) h.push(['esc', 'back']);
		if (app.details) h.push(['f', app.maximized ? 'restore' : 'maximize']);
		h.push(['?', 'help']);
		return h;
	});

	// The view's plugins, as k9s lists them in its menu; a key the view uses stays the view's.
	const pluginHints = $derived.by(() => {
		const taken = new Set(viewHints.map(([key]) => key));
		return (app.k9s?.plugins ?? [])
			.filter((p) => pluginApplies(p, app.pluginView))
			.flatMap((p): [string, string][] => {
				const shortcut = parseShortcut(p.shortCut);
				if (!shortcut) return [];
				const key = shortcutLabel(shortcut);
				return p.override || !taken.has(key) ? [[key, p.name]] : [];
			})
			.slice(0, 4);
	});

	/** Plays a hint's key, so that the hints work as buttons too. */
	function press(key: string) {
		const init = keyFromLabel(key);
		if (init) window.dispatchEvent(new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true }));
	}

	/** Keeps the focus where it is, so that the keys work on the list after a click. */
	const keepFocus = (e: MouseEvent) => e.preventDefault();

	// Help stays last.
	const hints = $derived([...viewHints.slice(0, -1), ...pluginHints, ...viewHints.slice(-1)]);

	const dark = $derived(
		app.scheme ? app.scheme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches
	);

	function signOut() {
		if (!drafts.block(() => session.signOut())) session.signOut();
	}

	let environmentEditor = $state<HTMLDetailsElement>();

	// A clock for the unlock countdown.
	let now = $state(Date.now());
	$effect(() => {
		if (app.unlockedUntil === null) return;
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});
	const unlockLeft = $derived(app.unlockedUntil === null ? null : Math.max(0, Math.ceil((app.unlockedUntil - now) / 1000)));
	const lockLabel = $derived(
		unlockLeft !== null
			? `Unlocked ${Math.floor(unlockLeft / 60)}:${String(unlockLeft % 60).padStart(2, '0')}`
			: app.readOnly
				? app.guard.canUnlock && !app.contextSettings.readOnly
					? 'Locked'
					: 'Read-only'
				: 'Editable'
	);
	const lockTitle = $derived(
		app.k9s?.readOnly
			? 'k9s readOnly is set: Kubri does not change this cluster'
			: app.contextSettings.readOnly
				? 'Read-only in Kubri: no edits, deletes, scaling or shells. Click to allow changes.'
				: unlockLeft !== null
					? 'Unlocked for now; it locks again by itself. Click to lock it now.'
					: app.guard.readOnly && app.guard.canUnlock
						? 'Locked: click to unlock for 15 minutes.'
						: app.guard.readOnly
							? `Read-only: ${app.guard.reason ?? 'Kubri does not change this cluster'}`
							: 'Changes are allowed; Kubernetes permissions still apply. Click to make Kubri read-only.'
	);
	const accent = $derived(app.contextSettings.color || null);
</script>

<svelte:window {onkeydown} {onmouseup} />

<div
	class="shell"
	class:maximized={app.details && app.maximized}
	class:inspecting={app.details}
	class:tinted={accent}
	style:--context-color={accent}
>
	<Sidebar />
	<div class="workspace">
		<header>
			<div class="brand">
				<img src={logo} alt="" width="24" height="24" />
				<span>kubri</span>
			</div>
			<div class="history">
				<button
					onclick={() => app.go(-1)}
					onmousedown={keepFocus}
					disabled={!app.canGoBack}
					title="Back ([ or Alt+←)"
					aria-label="Back"
				>
					<Icon name="back" size={16} />
				</button>
				<button
					onclick={() => app.go(1)}
					onmousedown={keepFocus}
					disabled={!app.canGoForward}
					title="Forward (] or Alt+→)"
					aria-label="Forward"
				>
					<Icon name="next" size={16} />
				</button>
			</div>
			<ContextSwitcher />
			{#if app.cluster}
				<details class="environment" bind:this={environmentEditor}>
					<summary
						class:set={app.environment}
						class:production={app.production}
						title="Name, color and guard rails of this context"
					>
						{app.environment || 'Context settings'}
					</summary>
					<div class="popover">
						<p class="popover-title">Settings for <strong>{app.cluster.context}</strong></p>
						<ContextSettingsForm context={app.cluster.context} namespaces={app.namespaces} />
						<p class="note">Kept in Kubri's settings file; the kubeconfig is never changed.</p>
					</div>
				</details>
			{/if}
			<span class="divider"></span>
			<NamespacePicker />
			<button class="palette-button" onclick={() => (app.paletteOpen = true)} aria-label="Search or run a command">
				<Icon name="search" size={17} />
				<span>Search or run a command</span>
				<kbd>Ctrl K</kbd>
			</button>
			<button
				class="square"
				onclick={() => app.toggleScheme()}
				disabled={!!app.skin}
				title={app.skin ? `The skin ${app.skin} sets the colors` : dark ? 'Use light colors' : 'Use dark colors'}
				aria-label={dark ? 'Use light colors' : 'Use dark colors'}
			>
				<Icon name={dark ? 'sun' : 'moon'} />
			</button>
			<button
				class="mode"
				class:locked={app.readOnly}
				class:unlocked={app.unlockedUntil !== null}
				aria-pressed={app.readOnly}
				disabled={!app.cluster || app.k9s?.readOnly || (app.guard.readOnly && !app.guard.canUnlock && !app.contextSettings.readOnly)}
				onclick={() => app.toggleReadOnly()}
				title={lockTitle}
			>
				<Icon name={app.readOnly ? 'lock' : 'unlock'} size={15} />
				{lockLabel}
			</button>
			<button class="square help" onclick={() => (app.helpOpen = true)} title="Keys and commands (?)" aria-label="Help">
				?
			</button>
			{#if session.required}
				<button class="sign-out" onclick={signOut}>Sign out</button>
			{/if}
		</header>

		{#if app.cluster && app.production}
			<div class="production-banner" role="note">
				<Icon name="warning" size={15} />
				<span>
					<strong>Production</strong> · {app.contextName(app.cluster.context)} ·
					{app.readOnly ? 'changes are off' : 'changes go live; deleting asks you to type the name'}
				</span>
			</div>
		{/if}
		{#if app.settingsError}
			<div class="error" role="alert">
				<span>Kubri's settings file could not be read, so it uses the defaults: {app.settingsError}</span>
				<button onclick={() => (app.settingsError = null)} aria-label="Dismiss">×</button>
			</div>
		{/if}
		{#if app.error}
			<div class="error" role="alert">
				<span>{app.error}</span>
				<button onclick={() => (app.error = null)} aria-label="Dismiss">×</button>
			</div>
		{/if}

		<div class="area" class:right={dock.sessions.length && dock.side === 'right' && !dock.minimized}>
			<main
				class:with-details={app.details}
				class:wide={app.panelWide}
				style:--panel-width={app.panelWidth ? `${app.panelWidth}px` : null}
			>
				{#if app.page === 'fleet'}
					<FleetView />
				{:else if app.cluster && !app.connecting}
					{#if app.page === 'portforwards'}
						<PortForwardsView />
					{:else if app.page === 'benchmarks'}
						<BenchmarksView />
					{:else if app.page === 'search'}
						<SearchView />
					{:else if app.page === 'config'}
						<ConfigView />
					{:else if app.page === 'integrations'}
						{#key app.cluster?.context}
							<IntegrationsView />
						{/key}
					{:else if app.page === 'rightsizing'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<RightSizingView />
						{/key}
					{:else if app.page === 'sanitizer'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<SanitizerView />
						{/key}
					{:else if app.page === 'pulses'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<PulsesView />
						{/key}
					{:else if app.page === 'graph'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<GraphView />
						{/key}
					{:else if app.page === 'changes'}
						{#key `${app.cluster?.context}/${app.namespace}/${JSON.stringify(app.changesFilter)}`}
							<ChangesView />
						{/key}
					{:else if app.page === 'xray' && app.xrayResource}
						{#key `${app.cluster?.context}/${app.namespace}/${app.xrayResource.plural}`}
							<XrayView resource={app.xrayResource} />
						{/key}
					{:else if app.page === 'rbac'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<RbacView />
						{/key}
					{:else if app.page === 'create'}
					<CreateView />
				{:else if app.page === 'helm'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<HelmView />
						{/key}
					{:else if app.selected}
						{#key `${app.cluster?.context}/${app.selected.group}/${app.selected.version}/${app.selected.plural}/${JSON.stringify(app.scope)}`}
							<ResourceView resource={app.selected} namespace={app.namespace} scope={app.scope} />
						{/key}
					{:else}
						<div class="placeholder">Choose a resource or press <kbd>:</kbd>.</div>
					{/if}
				{:else}
					<section class="connect">
						<img src={logo} alt="" width="56" height="56" />
						<span class="eyebrow">Your cluster, in focus</span>
						<h1>{app.connecting ? `Connecting to ${app.connecting}…` : 'Connect to a cluster.'}</h1>
						<p>
							{isTauri()
								? 'Choose a context from your kubeconfig. Reload it after you update your credentials.'
								: 'Choose a context configured on kubri-server. The server holds the credentials.'}
						</p>
						{#if !app.connecting}
							<div class="connect-actions">
								<button
									class="primary"
									onclick={() => {
										app.contextMenuOpen = true;
										app.reloadContexts(true);
									}}
								>
									Choose context <Icon name="arrow" size={16} />
								</button>
								<button onclick={() => app.load()}>Try again</button>
								{#if app.contexts.length > 1}<button onclick={() => app.showFleet()}>See all clusters</button>{/if}
							</div>
						{/if}
						<p class="small">Your k9s aliases, hotkeys, plugins and skins keep working.</p>
					</section>
				{/if}
				{#if app.details}
					<DetailsPanel details={app.details} />
				{/if}
			</main>
			{#if dock.sessions.length}<Dock />{/if}
		</div>

		<footer>
			<span class="status" class:live={app.cluster && !app.connecting} role="status">
				<span class="dot"></span>
				{app.connecting ? `Connecting to ${app.connecting}…` : app.cluster ? `Live · ${app.contextName(app.cluster.context)}` : 'Not connected'}
			</span>
			<span class="hints">
				{#each hints as [key, label], i (key + i)}
					{#if keyFromLabel(key)}
						<button onclick={() => press(key)} onmousedown={keepFocus} title="{label} ({key})"><kbd>{key}</kbd>{label}</button>
					{:else}
						<span><kbd>{key}</kbd>{label}</span>
					{/if}
				{/each}
			</span>
		</footer>
	</div>
</div>

{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.helpOpen}<HelpOverlay />{/if}
{#if app.menu}<ActionMenu menu={app.menu} />{/if}
<AgentRequests />
{#if app.dialog?.kind === 'forbidden'}
	<ForbiddenDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'kubectl'}
	<KubectlDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'debug'}
	<DebugDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'compare'}
	<CompareDialog dialog={app.dialog} />
{:else if app.dialog}
	<ActionDialog dialog={app.dialog} />
{/if}
{#if app.explaining}<ExplainPanel subject={app.explaining} />{/if}
{#if app.pluginTerminal}
	<PluginTerminal plugin={app.pluginTerminal.plugin} run={app.pluginTerminal.run} />
{/if}
<Toasts />

<style>
	.shell {
		display: grid;
		grid-template-columns: 220px minmax(0, 1fr);
		height: 100dvh;
	}
	/* Logs and shells can take the whole window. */
	.shell.maximized {
		grid-template-columns: minmax(0, 1fr);
	}
	.shell.maximized > :global(nav) {
		display: none;
	}
	.workspace {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 64px;
		padding: 8px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-1);
	}
	/* The sidebar shows the brand; the header takes it over when the sidebar is hidden. */
	.brand {
		display: none;
		align-items: center;
		gap: 7px;
		font-size: 21px;
		font-weight: 700;
		letter-spacing: -0.045em;
	}
	.history {
		display: flex;
		gap: 2px;
	}
	.history button,
	.square {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		flex: none;
		padding: 0;
		border: 1px solid transparent;
		border-radius: 8px;
		background: none;
		color: var(--text);
	}
	.history button:hover:not(:disabled),
	.square:hover:not(:disabled) {
		background: var(--surface-2);
	}
	.square {
		border-color: var(--border);
	}
	.help {
		display: none;
		font-weight: 700;
	}
	.environment {
		position: relative;
		flex: none;
	}
	.environment summary {
		padding: 3px 7px;
		border: 1px dashed var(--border);
		border-radius: 5px;
		font-size: 10.5px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		list-style: none;
		cursor: pointer;
		color: var(--text-muted);
	}
	.environment summary::-webkit-details-marker {
		display: none;
	}
	.environment summary.set {
		border: 1px solid transparent;
		background: color-mix(in srgb, var(--warn) 16%, var(--surface-1));
		color: var(--warn);
	}
	.environment summary.production {
		background: color-mix(in srgb, var(--bad) 16%, var(--surface-1));
		color: var(--bad);
	}
	.environment .popover {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		z-index: 20;
		display: grid;
		gap: 12px;
		width: 320px;
		max-height: calc(100dvh - 100px);
		overflow: auto;
		padding: 14px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
	}
	.popover-title {
		margin: 0;
		font-size: 13px;
	}
	.environment .note {
		margin: 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	/* The context's color, as a strip over the header. */
	.shell.tinted header {
		box-shadow: inset 0 3px 0 var(--context-color);
		background: color-mix(in srgb, var(--context-color) 6%, var(--surface-1));
	}
	.production-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 20px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
		font-size: 12.5px;
	}
	.mode.unlocked {
		border-color: transparent;
		background: color-mix(in srgb, var(--warn) 16%, var(--surface-1));
		color: var(--warn);
	}
	.divider {
		width: 1px;
		height: 26px;
		flex: none;
		background: var(--border);
	}
	.palette-button {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-left: auto;
		width: 260px;
		min-width: 0;
		padding: 7px 8px 7px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		color: var(--text-muted);
		font-size: 13px;
	}
	.palette-button span {
		flex: 1;
		overflow: hidden;
		text-align: left;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.palette-button:hover {
		border-color: var(--accent);
	}
	.palette-button kbd {
		margin: 0;
	}
	.mode {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.mode.locked {
		border-color: transparent;
		background: color-mix(in srgb, var(--ok) 14%, var(--surface-1));
		color: var(--ok);
	}
	.mode:hover:not(:disabled) {
		border-color: var(--accent);
	}
	.sign-out {
		flex: none;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: none;
		color: var(--text-muted);
		font-size: 12px;
	}
	.sign-out:hover {
		border-color: var(--accent);
		color: var(--text);
	}
	.error {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 20px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
	}
	.error button {
		border: none;
		background: none;
		font-size: 18px;
		line-height: 1;
	}
	/* The page, and the workspace below or beside it. */
	.area {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.area.right {
		flex-direction: row;
	}
	main {
		flex: 1;
		min-width: 0;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		min-height: 0;
	}
	/* A dragged width never leaves the list less than 320px. */
	main.with-details {
		grid-template-columns: minmax(0, 1fr) min(var(--panel-width, clamp(360px, 34vw, 540px)), calc(100% - 320px));
	}
	/* The list stays mounted so its shortcuts keep working. */
	main.wide {
		grid-template-columns: minmax(0, 1fr);
	}
	main.wide > :global(:not(aside)) {
		display: none;
	}
	.placeholder {
		display: grid;
		place-items: center;
		color: var(--text-muted);
	}
	.connect {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 32px 20px;
		text-align: center;
		overflow: auto;
	}
	.eyebrow {
		margin-top: 10px;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.connect h1 {
		margin: 0;
		font-size: 32px;
		letter-spacing: -0.03em;
	}
	.connect p {
		max-width: 460px;
		margin: 0;
		color: var(--text-muted);
		line-height: 1.6;
	}
	.connect .small {
		margin-top: 18px;
		font-size: 12px;
	}
	.connect-actions {
		display: flex;
		gap: 8px;
		margin-top: 14px;
	}
	.connect-actions button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 9px 16px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		font-weight: 600;
	}
	.connect-actions .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 4px 16px;
		border-top: 1px solid var(--border);
		background: var(--surface-1);
		font-size: 12px;
		color: var(--text-muted);
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		white-space: nowrap;
	}
	.status .dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
	}
	.status.live {
		color: var(--ok);
	}
	.status.live .dot {
		background: var(--ok);
	}
	.hints {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 2px 4px;
		margin-left: auto;
		min-width: 0;
	}
	.hints > * {
		display: inline-flex;
		align-items: center;
		padding: 2px 6px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		color: inherit;
		font-size: 12px;
	}
	.hints > button:hover {
		border-color: var(--border);
		background: var(--surface-2);
		color: var(--text);
	}
	kbd {
		margin-right: 5px;
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
		color: var(--text);
	}

	@media (max-width: 1280px) {
		.palette-button {
			width: 36px;
			flex: none;
			justify-content: center;
			padding: 7px;
		}
		.palette-button span,
		.palette-button kbd {
			display: none;
		}
	}
	/* The inspector takes the place of the list and the sidebar. */
	@media (max-width: 1100px) {
		.shell.inspecting {
			grid-template-columns: minmax(0, 1fr);
		}
		.shell.inspecting > :global(nav) {
			display: none;
		}
		.shell.inspecting .brand {
			display: flex;
		}
		main.with-details {
			grid-template-columns: minmax(0, 1fr);
		}
		main.with-details > :global(:not(aside)) {
			display: none;
		}
	}
	@media (max-width: 900px) {
		.history,
		.divider {
			display: none;
		}
	}
	/* One column: the header, then the sidebar as a strip of links, then the page. */
	@media (max-width: 720px) {
		.shell {
			display: flex;
			flex-direction: column;
		}
		.workspace {
			display: contents;
		}
		.shell > :global(nav) {
			order: 1;
		}
		.error {
			order: 2;
		}
		.area {
			order: 3;
		}
		footer {
			order: 4;
		}
		header {
			flex-wrap: wrap;
			gap: 6px;
			min-height: 0;
			padding: 8px 12px;
		}
		.brand {
			display: flex;
			margin-right: auto;
		}
		.help {
			display: grid;
		}
		.palette-button {
			order: 5;
			flex: 1 1 100%;
			width: 100%;
			justify-content: flex-start;
			padding: 7px 8px 7px 12px;
		}
		.palette-button span {
			display: block;
		}
		.hints {
			display: none;
		}
		.connect h1 {
			font-size: 26px;
		}
	}
</style>

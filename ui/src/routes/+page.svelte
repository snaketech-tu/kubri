<script lang="ts">
	import { untrack } from 'svelte';
	import { forControl } from '#lib/keys.js';
	import logo from '#lib/assets/logo.svg';
	import { canDelete, canEdit, canRestart, canScale } from '#lib/actions.js';
	import { listenForBindings } from '#lib/bindings.js';
	import ActionDialog from '#lib/components/ActionDialog.svelte';
	import BenchmarksView from '#lib/components/BenchmarksView.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import ConfigView from '#lib/components/ConfigView.svelte';
	import ContextSwitcher from '#lib/components/ContextSwitcher.svelte';
	import DetailsPanel from '#lib/components/DetailsPanel.svelte';
	import HelmView from '#lib/components/HelmView.svelte';
	import HelpOverlay from '#lib/components/HelpOverlay.svelte';
	import NamespacePicker from '#lib/components/NamespacePicker.svelte';
	import PluginTerminal from '#lib/components/PluginTerminal.svelte';
	import PortForwardsView from '#lib/components/PortForwardsView.svelte';
	import PulsesView from '#lib/components/PulsesView.svelte';
	import RbacView from '#lib/components/RbacView.svelte';
	import ResourceView from '#lib/components/ResourceView.svelte';
	import SanitizerView from '#lib/components/SanitizerView.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import XrayView from '#lib/components/XrayView.svelte';
	import { parseShortcut, pluginApplies, shortcutLabel } from '#lib/k9s.js';
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
	import { session } from '#lib/session.svelte.js';
	import { app } from '#lib/state.svelte.js';
	import { LiveTable } from '#lib/watch.svelte.js';

	const namespaces = new LiveTable();

	$effect(() => {
		// Also after signing in again, when a message from before may still be up.
		app.error = null;
		app.load();
	});

	// k9s plugin and hotkey keys.
	$effect(() => listenForBindings());

	$effect(() => {
		if (!app.cluster) return;
		return namespaces.start({ group: '', version: 'v1', plural: 'namespaces' }, null);
	});

	$effect(() => {
		app.namespaces = namespaces.rows.map((r) => r.name).sort();
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
		if (app.page === 'portforwards') {
			return [
				[':', 'command'],
				['c', 'copy url'],
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
				['⇧r', 'scan again'],
				...panel,
				['?', 'help']
			] as [string, string][];
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
				['m', 'manifest'],
				['h', 'history'],
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

	// Help stays last.
	const hints = $derived([...viewHints.slice(0, -1), ...pluginHints, ...viewHints.slice(-1)]);
</script>

<svelte:window {onkeydown} {onmouseup} />

<div class="shell">
	<header>
		<div class="brand">
			<img src={logo} alt="" width="26" height="26" />
			<span>Kubri</span>
		</div>
		<ContextSwitcher />
		<NamespacePicker />
		{#if app.readOnly}
			<span class="read-only" title="Kubri does not change this cluster: no edits, deletes, scaling or shells">Read-only</span>
		{/if}
		<button class="palette-button" onclick={() => (app.paletteOpen = true)}>
			<span>Go to resource or command…</span>
			<kbd>:</kbd>
		</button>
		{#if session.required}
			<button class="sign-out" onclick={() => session.signOut()}>Sign out</button>
		{/if}
	</header>

	{#if app.error}
		<div class="error">
			<span>{app.error}</span>
			<button onclick={() => (app.error = null)} aria-label="Dismiss">×</button>
		</div>
	{/if}

	<main class:with-details={app.details} class:maximized={app.details && app.maximized}>
		<Sidebar />
		{#if app.page === 'portforwards'}
			<PortForwardsView />
		{:else if app.page === 'benchmarks'}
			<BenchmarksView />
		{:else if app.page === 'config'}
			<ConfigView />
		{:else if app.page === 'sanitizer'}
			{#key `${app.cluster?.context}/${app.namespace}`}
				<SanitizerView />
			{/key}
		{:else if app.page === 'pulses'}
			{#key `${app.cluster?.context}/${app.namespace}`}
				<PulsesView />
			{/key}
		{:else if app.page === 'xray' && app.xrayResource}
			{#key `${app.cluster?.context}/${app.namespace}/${app.xrayResource.plural}`}
				<XrayView resource={app.xrayResource} />
			{/key}
		{:else if app.page === 'rbac'}
			{#key `${app.cluster?.context}/${app.namespace}`}
				<RbacView />
			{/key}
		{:else if app.page === 'helm'}
			{#key `${app.cluster?.context}/${app.namespace}`}
				<HelmView />
			{/key}
		{:else if app.selected}
			{#key `${app.cluster?.context}/${app.selected.group}/${app.selected.version}/${app.selected.plural}/${JSON.stringify(app.scope)}`}
				<ResourceView resource={app.selected} namespace={app.namespace} scope={app.scope} />
			{/key}
		{:else}
			<div class="placeholder">
				{#if app.contexts.length === 0 && !app.error}
					Looking for your kubeconfig…
				{:else if !app.cluster}
					Pick a context to connect to a cluster.
				{:else}
					Pick a resource on the left or press <kbd>:</kbd>.
				{/if}
			</div>
		{/if}
		{#if app.details}
			<DetailsPanel details={app.details} />
		{/if}
	</main>

	<footer>
		{#each hints as [key, label], i (key + i)}
			<span><kbd>{key}</kbd>{label}</span>
		{/each}
	</footer>
</div>

{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.helpOpen}<HelpOverlay />{/if}
{#if app.dialog}<ActionDialog dialog={app.dialog} />{/if}
{#if app.pluginTerminal}
	<PluginTerminal plugin={app.pluginTerminal.plugin} run={app.pluginTerminal.run} />
{/if}
<Toasts />

<style>
	.shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
	}
	header {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-1);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 208px;
		font-size: 17px;
		font-weight: 700;
		letter-spacing: -0.01em;
	}
	.read-only {
		padding: 2px 9px;
		border: 1px solid color-mix(in srgb, var(--warn) 50%, transparent);
		border-radius: 999px;
		background: color-mix(in srgb, var(--warn) 12%, transparent);
		color: var(--warn);
		font-size: 12px;
		font-weight: 600;
		white-space: nowrap;
	}
	.palette-button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-left: auto;
		width: 280px;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 13px;
	}
	.palette-button:hover {
		border-color: var(--accent);
	}
	.sign-out {
		margin-left: -8px;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 13px;
	}
	.sign-out:hover {
		border-color: var(--accent);
		color: var(--text);
	}
	.error {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
	}
	.error button {
		border: none;
		background: none;
		font-size: 18px;
		line-height: 1;
	}
	main {
		flex: 1;
		display: grid;
		grid-template-columns: 240px minmax(0, 1fr);
		min-height: 0;
	}
	main.with-details {
		grid-template-columns: 240px minmax(0, 1fr) minmax(380px, 44%);
	}
	/* The list stays mounted so its shortcuts keep working. */
	main.maximized {
		grid-template-columns: minmax(0, 1fr);
	}
	main.maximized > :global(:not(aside)) {
		display: none;
	}
	.placeholder {
		display: grid;
		place-items: center;
		color: var(--text-muted);
	}
	footer {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
		padding: 6px 16px;
		border-top: 1px solid var(--border);
		background: var(--surface-1);
		font-size: 12px;
		color: var(--text-muted);
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
</style>

<script lang="ts">
	import { forControl } from '#lib/keys.js';
	import logo from '#lib/assets/logo.svg';
	import { canDelete, canEdit, canRestart, canScale } from '#lib/actions.js';
	import ActionDialog from '#lib/components/ActionDialog.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import ContextSwitcher from '#lib/components/ContextSwitcher.svelte';
	import DetailsPanel from '#lib/components/DetailsPanel.svelte';
	import HelpOverlay from '#lib/components/HelpOverlay.svelte';
	import NamespacePicker from '#lib/components/NamespacePicker.svelte';
	import PortForwardsView from '#lib/components/PortForwardsView.svelte';
	import PulsesView from '#lib/components/PulsesView.svelte';
	import ResourceView from '#lib/components/ResourceView.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import XrayView from '#lib/components/XrayView.svelte';
	import { canPortForward, hasLogs, isNodes, isPods } from '#lib/pods.js';
	import { app } from '#lib/state.svelte.js';
	import { LiveTable } from '#lib/watch.svelte.js';

	const namespaces = new LiveTable();

	$effect(() => {
		app.load();
	});

	$effect(() => {
		if (!app.cluster) return;
		return namespaces.start({ group: '', version: 'v1', plural: 'namespaces' }, null);
	});

	$effect(() => {
		app.namespaces = namespaces.rows.map((r) => r.name).sort();
	});

	// Port-forwards outlive context switches; keep the sidebar count current.
	$effect(() => {
		app.refreshForwards();
		const timer = setInterval(() => app.refreshForwards(), 5000);
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
		}
	}

	const hints = $derived.by(() => {
		const panel: [string, string][] = app.details ? [['f', app.maximized ? 'restore' : 'maximize'], ['esc', 'close']] : [];
		if (app.page === 'portforwards') {
			return [
				[':', 'command'],
				['c', 'copy url'],
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
		const r = app.selected;
		const h: [string, string][] = [
			[':', 'command'],
			['/', 'filter'],
			['d', 'describe'],
			['y', 'yaml']
		];
		if (!r) return h;
		if (hasLogs(r)) h.push(['l', 'logs']);
		if (isPods(r)) h.push(['s', 'shell'], ['a', 'attach']);
		if (canPortForward(r)) h.push(['⇧f', 'port-forward']);
		if (isNodes(r)) h.push(['c', 'cordon'], ['u', 'uncordon'], ['r', 'drain']);
		if (canEdit(r)) h.push(['e', 'edit']);
		if (canDelete(r)) h.push(['^d', 'delete']);
		if (canScale(r) && !isPods(r)) h.push(['s', 'scale']);
		if (canRestart(r)) h.push(['r', 'restart']);
		if (app.details) h.push(['f', app.maximized ? 'restore' : 'maximize']);
		h.push(['?', 'help']);
		return h;
	});
</script>

<svelte:window {onkeydown} />

<div class="shell">
	<header>
		<div class="brand">
			<img src={logo} alt="" width="26" height="26" />
			<span>Kubri</span>
		</div>
		<ContextSwitcher />
		<NamespacePicker />
		<button class="palette-button" onclick={() => (app.paletteOpen = true)}>
			<span>Go to resource or command…</span>
			<kbd>:</kbd>
		</button>
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
		{:else if app.page === 'pulses'}
			{#key `${app.cluster?.context}/${app.namespace}`}
				<PulsesView />
			{/key}
		{:else if app.page === 'xray' && app.xrayResource}
			{#key `${app.cluster?.context}/${app.namespace}/${app.xrayResource.plural}`}
				<XrayView resource={app.xrayResource} />
			{/key}
		{:else if app.selected}
			{#key `${app.cluster?.context}/${app.selected.group}/${app.selected.version}/${app.selected.plural}`}
				<ResourceView resource={app.selected} namespace={app.namespace} />
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
		{#each hints as [key, label] (key)}
			<span><kbd>{key}</kbd>{label}</span>
		{/each}
	</footer>
</div>

{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.helpOpen}<HelpOverlay />{/if}
{#if app.dialog}<ActionDialog dialog={app.dialog} />{/if}
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

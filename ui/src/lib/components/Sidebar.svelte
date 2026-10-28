<script lang="ts">
	import type { ResourceInfo } from '../api';
	import { app, sameResource } from '../state.svelte';
	import { xrayResources } from '../xray';

	// Shown first, in this order, like the views k9s users jump to most.
	const PINNED = [
		'pods', 'deployments', 'statefulsets', 'daemonsets', 'jobs', 'cronjobs',
		'services', 'ingresses', 'configmaps', 'secrets', 'persistentvolumeclaims',
		'nodes', 'namespaces', 'events'
	];

	let filter = $state('');

	const visible = $derived(
		app.resources.filter(
			(r) =>
				r.verbs.includes('watch') &&
				(r.kind.toLowerCase().includes(filter.toLowerCase()) ||
					r.plural.includes(filter.toLowerCase()) ||
					r.shortNames.includes(filter.toLowerCase()))
		)
	);
	const pinned = $derived(
		// Resources are sorted with the core group first, so core events win over events.k8s.io.
		PINNED.map((plural) => visible.find((r) => r.plural === plural)).filter(
			(r): r is ResourceInfo => !!r
		)
	);
	const others = $derived(visible.filter((r) => !pinned.includes(r)));

	const running = $derived(app.benchmarks.filter((b) => b.state === 'running').length);

	function showXray() {
		const resource = app.xrayResource ?? xrayResources(app.resources)[0];
		if (resource) app.showXray(resource);
	}
</script>

<nav>
	<input class="filter" bind:value={filter} placeholder="Filter resources…" />
	<div class="scroll">
		<h3>Tools</h3>
		<button class:active={app.page === 'pulses'} onclick={() => app.showPulses()} title=":pulses">
			<span>Pulses</span>
		</button>
		<button class:active={app.page === 'xray'} onclick={showXray} title=":xray deploy">
			<span>XRay</span>
		</button>
		<button class:active={app.page === 'rbac'} onclick={() => app.showRbac()} title=":rbac">
			<span>RBAC</span>
		</button>
		<button class:active={app.page === 'helm'} onclick={() => app.showHelm()} title=":helm">
			<span>Helm</span>
		</button>
		<button class:active={app.page === 'sanitizer'} onclick={() => app.showSanitizer()} title=":popeye">
			<span>Sanitizer</span>
		</button>
		<button class:active={app.page === 'portforwards'} onclick={() => app.showPortForwards()} title=":pf">
			<span>Port-forwards</span>
			{#if app.forwards.length}<span class="badge">{app.forwards.length}</span>{/if}
		</button>
		<button class:active={app.page === 'benchmarks'} onclick={() => app.showBenchmarks()} title=":bench">
			<span>Benchmarks</span>
			{#if running}<span class="badge" title="Running">{running}</span>{/if}
		</button>
		{#if pinned.length}
			<h3>Workloads & config</h3>
			{#each pinned as r (r.group + r.plural)}
				{@render item(r)}
			{/each}
		{/if}
		{#if others.length}
			<h3>All resources</h3>
			{#each others as r (r.group + '/' + r.version + '/' + r.plural)}
				{@render item(r)}
			{/each}
		{/if}
	</div>
</nav>

{#snippet item(r: ResourceInfo)}
	<button
		class:active={app.page === 'resources' && app.selected && sameResource(r, app.selected)}
		onclick={() => app.open(r)}
		title={r.shortNames.length ? `:${r.shortNames[0]}` : `:${r.plural}`}
	>
		<span>{r.kind}</span>
		{#if r.group}<span class="group">{r.group}</span>{/if}
	</button>
{/snippet}

<style>
	nav {
		display: flex;
		flex-direction: column;
		min-height: 0;
		border-right: 1px solid var(--border);
		background: var(--surface-1);
	}
	.filter {
		margin: 12px;
	}
	.scroll {
		overflow-y: auto;
		padding: 0 8px 12px;
	}
	h3 {
		margin: 12px 8px 4px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	button {
		display: flex;
		align-items: baseline;
		gap: 6px;
		width: 100%;
		padding: 5px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		text-align: left;
	}
	button:hover {
		background: var(--surface-2);
	}
	button.active {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		color: var(--accent-strong);
		font-weight: 600;
	}
	.badge {
		margin-left: auto;
		padding: 0 7px;
		border-radius: 999px;
		background: var(--accent);
		color: white;
		font-size: 11px;
		font-weight: 600;
	}
	.group {
		font-size: 11px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>

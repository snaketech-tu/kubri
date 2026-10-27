<script lang="ts">
	import type { ResourceInfo } from '../api';
	import { app, sameResource } from '../state.svelte';

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
					r.plural.includes(filter.toLowerCase()))
		)
	);
	const pinned = $derived(
		// Resources are sorted with the core group first, so core events win over events.k8s.io.
		PINNED.map((plural) => visible.find((r) => r.plural === plural)).filter(
			(r): r is ResourceInfo => !!r
		)
	);
	const others = $derived(visible.filter((r) => !pinned.includes(r)));
</script>

<nav>
	<input class="filter" bind:value={filter} placeholder="Filter resources…" />
	<div class="scroll">
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
	<button class:active={app.selected && sameResource(r, app.selected)} onclick={() => (app.selected = r)}>
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
	.group {
		font-size: 11px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>

<script lang="ts">
	import { backend, type Dashboards, type ObjectRef } from '../api';
	import { errorMessage } from '../format';
	import { openTool } from '../integrations';
	import Icon from './Icon.svelte';

	let {
		target,
		title = 'Grafana',
		empty = 'Grafana has no dashboard for this object.'
	}: { target: ObjectRef; title?: string; empty?: string } = $props();

	let found = $state<Dashboards | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		const current = target;
		let cancelled = false;
		found = null;
		error = null;
		backend.dashboards(current).then(
			(d) => !cancelled && (found = d),
			(err) => !cancelled && (error = errorMessage(err))
		);
		return () => {
			cancelled = true;
		};
	});
</script>

<div class="dashboards">
	<strong>{title}</strong>
	{#if error}
		<p class="muted">{error}</p>
	{:else if !found}
		<p class="muted">Looking for dashboards…</p>
	{:else if !found.links.length}
		<p class="muted">{empty}{found.note ? ` Grafana's own list: ${found.note}` : ''}</p>
	{:else}
		<ul>
			{#each found.links as link (link.path)}
				<li>
					<button class="link" onclick={() => openTool('grafana', link.path)} title="Open in Grafana with {link.filled.join(', ')} set">
						<Icon name="popout" size={13} />
						{link.title}
					</button>
					{#if link.folder}<span class="muted">{link.folder}</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.dashboards {
		font-size: 12.5px;
	}
	ul {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin: 4px 0 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		text-align: left;
	}
	.muted {
		margin: 4px 0 0;
		color: var(--text-muted);
	}
	li .muted {
		margin: 0;
		font-size: 11.5px;
	}
</style>

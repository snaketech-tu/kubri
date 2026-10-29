<script lang="ts">
	import type { EventInfo } from '../api';
	import { age } from '../format';

	let {
		events,
		now,
		limit = Infinity
	}: {
		events: EventInfo[];
		now: number;
		/** Shows only the latest ones. */
		limit?: number;
	} = $props();

	const latest = $derived(
		[...events].sort((a, b) => (b.lastSeen ?? '').localeCompare(a.lastSeen ?? '')).slice(0, limit)
	);
</script>

{#if latest.length}
	<ol class="timeline">
		{#each latest as e, i (i)}
			<li class:warning={e.kind === 'Warning'}>
				<span class="dot" aria-hidden="true"></span>
				<div>
					<strong>{e.reason}</strong>
					{#if e.kind === 'Warning'}<span class="tag">Warning</span>{/if}
					<p class="meta">
						{age(e.lastSeen, now) || 'just now'} ago{e.count > 1 ? ` · ${e.count} times` : ''}{e.source
							? ` · ${e.source}`
							: ''}
					</p>
					<p class="message">{e.message}</p>
				</div>
			</li>
		{/each}
	</ol>
{:else}
	<p class="none">No recent events.</p>
{/if}

<style>
	.timeline {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		position: relative;
		display: flex;
		gap: 12px;
		padding: 0 0 14px;
	}
	/* The line between the dots. */
	li:not(:last-child)::before {
		content: '';
		position: absolute;
		top: 12px;
		bottom: 0;
		left: 3.5px;
		width: 1px;
		background: var(--border);
	}
	.dot {
		width: 8px;
		height: 8px;
		margin-top: 5px;
		flex: none;
		border-radius: 50%;
		background: var(--text-muted);
	}
	.warning .dot {
		background: var(--bad);
	}
	div {
		min-width: 0;
	}
	strong {
		font-size: 13px;
	}
	.tag {
		margin-left: 6px;
		padding: 0 6px;
		border-radius: 4px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
		font-size: 10.5px;
		font-weight: 600;
	}
	p {
		margin: 2px 0 0;
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	.meta,
	.none {
		color: var(--text-muted);
	}
	.none {
		margin: 0;
		font-size: 13px;
	}
</style>

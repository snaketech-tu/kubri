<script lang="ts">
	import type { Bucket } from '../logs';
	import { shortTime } from '../logs';

	let { buckets, onjump }: { buckets: Bucket[]; onjump: (row: number) => void } = $props();

	const HEIGHT = 34;
	let width = $state(600);
	let hover = $state<number | null>(null);

	const max = $derived(Math.max(1, ...buckets.map((b) => b.lines)));
	/** The strip's width inside its padding. */
	const inner = $derived(Math.max(60, width - 32));
	const slot = $derived(inner / Math.max(1, buckets.length));
	const h = (n: number) => (n ? Math.max(2, (n / max) * (HEIGHT - 4)) : 0);

	function label(b: Bucket) {
		const parts = [`${shortTime(new Date(b.from).toISOString())}: ${b.lines} ${b.lines === 1 ? 'line' : 'lines'}`];
		if (b.errors) parts.push(`${b.errors} ${b.errors === 1 ? 'error' : 'errors'}`);
		if (b.warnings) parts.push(`${b.warnings} ${b.warnings === 1 ? 'warning' : 'warnings'}`);
		return [parts.join(', '), ...b.markers].join('\n');
	}

	const hovered = $derived(hover === null ? null : buckets[hover]);
</script>

<div class="rate" bind:clientWidth={width}>
	<svg width={inner} height={HEIGHT} role="img" aria-label="Log lines over time; click a column to jump to its lines">
		{#each buckets as b, i (i)}
			{@const x = i * slot}
			{@const w = Math.max(1, slot - 1)}
			{@const total = h(b.lines)}
			{@const bad = b.lines ? (total * b.errors) / b.lines : 0}
			{@const warn = b.lines ? (total * b.warnings) / b.lines : 0}
			{#if b.markers.length}<rect class="marker" x={x + w / 2 - 1} y="0" width="2" height={HEIGHT} />{/if}
			<rect class="other" {x} y={HEIGHT - total} width={w} height={total - bad - warn} />
			<rect class="warn" {x} y={HEIGHT - bad - warn} width={w} height={warn} />
			<rect class="bad" {x} y={HEIGHT - bad} width={w} height={bad} />
			<rect
				class="hit"
				class:on={hover === i}
				{x}
				y="0"
				width={slot}
				height={HEIGHT}
				role="presentation"
				onpointerenter={() => (hover = i)}
				onpointerleave={() => (hover = null)}
				onclick={() => b.first >= 0 && onjump(b.first)}
			/>
		{/each}
	</svg>
	{#if hovered}
		<div class="tip" style:left="{Math.max(0, Math.min(width - 220, (hover ?? 0) * slot - 94))}px">{label(hovered)}</div>
	{/if}
</div>

<style>
	.rate {
		position: relative;
		padding: 4px 16px 2px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border-subtle);
	}
	svg {
		display: block;
		width: 100%;
	}
	.other {
		fill: color-mix(in srgb, var(--text-muted) 45%, transparent);
	}
	.warn {
		fill: var(--warn);
	}
	.bad {
		fill: var(--bad);
	}
	.marker {
		fill: var(--accent);
		opacity: 0.7;
	}
	.hit {
		fill: transparent;
		cursor: pointer;
	}
	.hit.on {
		fill: color-mix(in srgb, var(--accent) 12%, transparent);
	}
	.tip {
		position: absolute;
		top: 40px;
		z-index: 3;
		width: 220px;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		font-size: 11.5px;
		white-space: pre-line;
		pointer-events: none;
	}
</style>

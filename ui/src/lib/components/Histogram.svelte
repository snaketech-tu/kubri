<script lang="ts">
	import { duration, ticks } from '../bench';

	let { buckets, total }: { buckets: { from: number; to: number; count: number }[]; total: number } = $props();

	const HEIGHT = 170;
	const PAD = { top: 10, right: 8, bottom: 24, left: 36 };

	let width = $state(480);
	let hover = $state<number | null>(null);

	const plotW = $derived(Math.max(60, width - PAD.left - PAD.right));
	const plotH = HEIGHT - PAD.top - PAD.bottom;
	const max = $derived(Math.max(1, ...buckets.map((b) => b.count)));
	const yTicks = $derived(ticks(max));
	const yMax = $derived(yTicks[yTicks.length - 1] || 1);
	const slot = $derived(plotW / Math.max(1, buckets.length));
	// Columns stay thin and leave air in their slot; at least a 2px gap between neighbours.
	const barW = $derived(Math.max(2, Math.min(24, slot - 2)));
	const y = (v: number) => PAD.top + plotH - (v / yMax) * plotH;
	// Label the first, middle and last bucket edges only.
	const xLabels = $derived.by(() => {
		if (!buckets.length) return [];
		const last = buckets.length;
		const edge = (i: number) => (i === last ? buckets[last - 1].to : buckets[i].from);
		return [0, Math.round(last / 2), last].map((i) => ({ x: PAD.left + i * slot, text: duration(edge(i)) }));
	});

	/** A column with a 4px rounded top and a square foot on the baseline. */
	function column(x: number, top: number, w: number) {
		const bottom = PAD.top + plotH;
		// Keep a sliver of a non-empty bucket visible next to a tall one.
		const h = Math.max(2, bottom - top);
		top = bottom - h;
		const r = Math.min(4, w / 2, h);
		return `M${x} ${bottom}V${top + r}Q${x} ${top} ${x + r} ${top}H${x + w - r}Q${x + w} ${top} ${x + w} ${top + r}V${bottom}Z`;
	}

	function compact(n: number) {
		return n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}K` : String(n);
	}
</script>

<div class="chart" bind:clientWidth={width}>
	<svg width={width} height={HEIGHT} role="img" aria-label="Response time distribution of {total} responses">
		{#each yTicks as t (t)}
			<line class="grid" x1={PAD.left} x2={PAD.left + plotW} y1={y(t)} y2={y(t)} />
			<text class="axis" x={PAD.left - 6} y={y(t)} dy="0.32em" text-anchor="end">{compact(t)}</text>
		{/each}
		{#each buckets as b, i (i)}
			{@const x = PAD.left + i * slot + (slot - barW) / 2}
			{#if b.count}<path class="bar" class:dim={hover !== null && hover !== i} d={column(x, y(b.count), barW)} />{/if}
			<rect
				class="hit"
				x={PAD.left + i * slot}
				y={PAD.top}
				width={slot}
				height={plotH}
				role="presentation"
				onpointerenter={() => (hover = i)}
				onpointerleave={() => (hover = null)}
			/>
		{/each}
		{#each xLabels as l, i (i)}
			<text class="axis" x={l.x} y={HEIGHT - 6} text-anchor={i === 0 ? 'start' : i === 2 ? 'end' : 'middle'}>{l.text}</text>
		{/each}
	</svg>
	{#if hover !== null && buckets[hover]}
		{@const b = buckets[hover]}
		<div
			class="tip"
			style:left="{Math.min(width - 170, Math.max(0, PAD.left + hover * slot + slot / 2 - 85))}px"
			style:top="{Math.max(0, y(b.count) - 52)}px"
		>
			<strong>{b.count.toLocaleString('en-US')}</strong>
			{b.count === 1 ? 'response' : 'responses'} · {total ? Math.round((100 * b.count) / total) : 0}%
			<div class="muted">{duration(b.from)} – {duration(b.to)}</div>
		</div>
	{/if}
</div>

<style>
	.chart {
		position: relative;
		width: 100%;
	}
	svg {
		display: block;
	}
	.grid {
		stroke: var(--border-subtle);
		stroke-width: 1;
	}
	.axis {
		fill: var(--text-muted);
		font-size: 10.5px;
		font-variant-numeric: tabular-nums;
	}
	.bar {
		fill: var(--accent);
		transition: opacity 0.1s;
	}
	.bar.dim {
		opacity: 0.45;
	}
	.hit {
		fill: transparent;
	}
	.tip {
		position: absolute;
		width: 170px;
		padding: 6px 9px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		font-size: 12px;
		pointer-events: none;
	}
	.muted {
		color: var(--text-muted);
	}
</style>

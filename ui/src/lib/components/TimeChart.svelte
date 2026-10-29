<script lang="ts">
	import type { Chart } from '../api';
	import { colorSlots, formatValue, timeLabel, timeTicks, valueAt, valueTicks } from '../charts';

	let {
		chart,
		start,
		end,
		step,
		markers = []
	}: {
		chart: Chart;
		start: number;
		end: number;
		step: number;
		markers?: { at: number; label: string }[];
	} = $props();

	const HEIGHT = 150;
	const LEFT = 52;
	const RIGHT = 10;
	const TOP = 8;
	const BOTTOM = 20;

	let width = $state(0);
	/** The time under the pointer. */
	let hover = $state<number | null>(null);

	const slots = $derived(colorSlots(chart.series));
	const color = (name: string) => {
		const slot = slots.get(name);
		return slot ? `var(--series-${slot})` : 'var(--text-muted)';
	};
	const max = $derived(Math.max(...chart.series.flatMap((s) => s.points.map((p) => p[1])), ...chart.levels.map((l) => l.value), 0));
	const ticks = $derived(valueTicks(max, chart.unit));
	const top = $derived(ticks[ticks.length - 1] || 1);
	const times = $derived(timeTicks(start, end));
	const plotWidth = $derived(Math.max(0, width - LEFT - RIGHT));
	const x = (at: number) => LEFT + ((at - start) / (end - start || 1)) * plotWidth;
	const y = (value: number) => TOP + (1 - value / top) * (HEIGHT - TOP - BOTTOM);

	/** Each line as a path; a gap in the data breaks the line rather than bridging it. */
	function path(points: [number, number][]) {
		let d = '';
		let last: number | null = null;
		for (const [at, value] of points) {
			const move = last === null || at - last > step * 1.5;
			d += `${move ? 'M' : 'L'}${x(at).toFixed(1)},${y(value).toFixed(1)}`;
			last = at;
		}
		return d;
	}

	const single = $derived(chart.series.length === 1);
	const rows = $derived(
		hover === null
			? []
			: chart.series
					.map((s) => ({ name: s.name, value: valueAt(s, hover!, step) }))
					.filter((r) => r.value !== null)
					.sort((a, b) => b.value! - a.value!)
	);
	const latest = (s: Chart['series'][number]) => s.points[s.points.length - 1]?.[1];

	function onpointermove(e: PointerEvent) {
		const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
		const px = e.clientX - rect.left;
		if (px < LEFT || px > LEFT + plotWidth) {
			hover = null;
			return;
		}
		const at = start + ((px - LEFT) / plotWidth) * (end - start);
		hover = start + Math.round((at - start) / step) * step;
	}

	const markerNear = $derived(
		hover === null ? null : markers.find((m) => Math.abs(x(m.at) - x(hover!)) < 6)
	);
	const tipLeft = $derived(hover !== null && x(hover) > width / 2);
</script>

<figure class="chart">
	<figcaption>
		<strong>{chart.title}</strong>
		{#if single && latest(chart.series[0]) !== undefined}
			<span class="now">{formatValue(chart.unit, latest(chart.series[0])!)} now</span>
		{/if}
	</figcaption>
	<div class="plot" bind:clientWidth={width}>
		{#if width}
			<svg
				{width}
				height={HEIGHT}
				role="img"
				aria-label="{chart.title}: {chart.series.length} {chart.series.length === 1 ? 'line' : 'lines'}"
				{onpointermove}
				onpointerleave={() => (hover = null)}
			>
				{#each ticks as tick (tick)}
					<line class="grid" x1={LEFT} x2={LEFT + plotWidth} y1={y(tick)} y2={y(tick)} />
					<text class="axis" x={LEFT - 6} y={y(tick)} text-anchor="end" dominant-baseline="middle">{formatValue(chart.unit, tick)}</text>
				{/each}
				{#each times as at (at)}
					<text class="axis" x={x(at)} y={HEIGHT - 5} text-anchor="middle">{timeLabel(at, end - start)}</text>
				{/each}
				{#each markers as marker (marker.at + marker.label)}
					<line class="marker" x1={x(marker.at)} x2={x(marker.at)} y1={TOP} y2={HEIGHT - BOTTOM}>
						<title>{marker.label}</title>
					</line>
				{/each}
				{#each chart.levels as level (level.label)}
					<line class="level" x1={LEFT} x2={LEFT + plotWidth} y1={y(level.value)} y2={y(level.value)} />
					<text class="level-label" x={LEFT + plotWidth - 2} y={y(level.value) - 4} text-anchor="end">
						{level.label} {formatValue(chart.unit, level.value)}
					</text>
				{/each}
				{#each chart.series as s (s.name)}
					<path class="line" d={path(s.points)} style:stroke={color(s.name)} />
				{/each}
				{#if hover !== null}
					<line class="cross" x1={x(hover)} x2={x(hover)} y1={TOP} y2={HEIGHT - BOTTOM} />
					{#each chart.series as s (s.name)}
						{@const value = valueAt(s, hover, step)}
						{#if value !== null}
							<circle cx={x(hover)} cy={y(value)} r="4" style:fill={color(s.name)} />
						{/if}
					{/each}
				{/if}
			</svg>
			{#if hover !== null && (rows.length || markerNear)}
				<div class="tip" style:left={tipLeft ? null : `${x(hover) + 12}px`} style:right={tipLeft ? `${width - x(hover) + 12}px` : null}>
					<div class="when">{new Date(hover * 1000).toLocaleString(undefined, { weekday: 'short', hour: '2-digit', minute: '2-digit' })}</div>
					{#if markerNear}<div class="event">{markerNear.label}</div>{/if}
					{#each rows.slice(0, 8) as row (row.name)}
						<div class="row">
							<span class="swatch" style:background={color(row.name)}></span>
							<span class="name">{single ? chart.title : row.name}</span>
							<strong>{formatValue(chart.unit, row.value!)}</strong>
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
	{#if !single}
		<ul class="legend">
			{#each chart.series as s (s.name)}
				{@const value = latest(s)}
				<li>
					<span class="swatch" style:background={color(s.name)}></span>
					<span class="name" title={s.name}>{s.name}</span>
					{#if value !== undefined}<span class="value">{formatValue(chart.unit, value)}</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</figure>

<style>
	.chart {
		margin: 0;
		min-width: 0;
	}
	figcaption {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: 4px;
		font-size: 12.5px;
	}
	.now {
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}
	.plot {
		position: relative;
		min-width: 0;
	}
	svg {
		display: block;
		overflow: visible;
		cursor: crosshair;
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
	.line {
		fill: none;
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.level {
		stroke: var(--text-muted);
		stroke-width: 1;
		stroke-dasharray: 4 3;
	}
	.level-label {
		fill: var(--text-muted);
		font-size: 10.5px;
	}
	.marker {
		stroke: var(--text-muted);
		stroke-width: 1;
		stroke-dasharray: 2 3;
		opacity: 0.8;
	}
	.cross {
		stroke: var(--text-muted);
		stroke-width: 1;
	}
	circle {
		stroke: var(--surface-1);
		stroke-width: 2;
	}
	.tip {
		position: absolute;
		top: 4px;
		z-index: 5;
		min-width: 150px;
		max-width: 280px;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		font-size: 11.5px;
		pointer-events: none;
	}
	.when {
		color: var(--text-muted);
		margin-bottom: 2px;
	}
	.event {
		margin-bottom: 2px;
		font-weight: 600;
	}
	.row,
	.legend li {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}
	.row strong,
	.legend .value {
		margin-left: auto;
		font-variant-numeric: tabular-nums;
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.swatch {
		flex: none;
		width: 10px;
		height: 3px;
		border-radius: 2px;
	}
	.legend {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 2px 14px;
		margin: 6px 0 0;
		padding: 0 0 0 52px;
		list-style: none;
		color: var(--text-muted);
		font-size: 11.5px;
	}
	.legend .value {
		color: var(--text);
	}
</style>

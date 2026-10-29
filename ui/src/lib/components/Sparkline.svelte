<script lang="ts">
	import { HISTORY_MS, type Sample } from '../pulse';

	let {
		samples,
		format,
		label,
		now,
		window = HISTORY_MS
	}: {
		samples: Sample[];
		format: (value: number) => string;
		label: string;
		now: number;
		/** How far back the line goes, in milliseconds. */
		window?: number;
	} = $props();

	const HEIGHT = 40;
	const PAD = 5;
	/** The line fills the width from the start, then scrolls once it covers the history. */
	const MIN_SPAN_MS = 2 * 60 * 1000;

	let width = $state(0);
	let hover = $state<number | null>(null);

	const span = $derived(Math.min(window, Math.max(MIN_SPAN_MS, samples.length ? now - samples[0].at : 0)));
	// Zero stays at the bottom so the line does not overstate small changes.
	const top = $derived(Math.max(...samples.map((s) => s.value), 1) * 1.2);
	const x = (at: number) => PAD + ((at - (now - span)) / span) * (width - 2 * PAD);
	const y = (value: number) => HEIGHT - PAD - (value / top) * (HEIGHT - 2 * PAD);
	const points = $derived(samples.map((s) => [x(s.at), y(s.value)] as const));
	const line = $derived(points.map(([px, py], i) => `${i ? 'L' : 'M'}${px.toFixed(1)},${py.toFixed(1)}`).join(''));
	const area = $derived(
		points.length > 1
			? `${line}L${points[points.length - 1][0].toFixed(1)},${HEIGHT - PAD}L${points[0][0].toFixed(1)},${HEIGHT - PAD}Z`
			: ''
	);
	const shown = $derived(hover ?? samples.length - 1);
	const point = $derived(points[shown]);
	const minutes = $derived(Math.round(span / 60_000));

	function agoText(at: number) {
		const seconds = Math.round((now - at) / 1000);
		if (seconds < 5) return 'now';
		if (seconds < 60) return `${seconds}s ago`;
		return seconds < 600 ? `${Math.floor(seconds / 60)}m ${seconds % 60}s ago` : `${Math.round(seconds / 60)}m ago`;
	}

	// The crosshair snaps to the nearest sample; the pointer never has to hit the line.
	function onpointermove(e: PointerEvent) {
		if (!points.length) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const px = e.clientX - rect.left;
		let best = 0;
		points.forEach(([p], i) => {
			if (Math.abs(p - px) < Math.abs(points[best][0] - px)) best = i;
		});
		hover = best;
	}
</script>

<!-- The latest value is also written out next to the chart, so hovering only adds history. -->
<div class="spark" bind:clientWidth={width} {onpointermove} onpointerleave={() => (hover = null)} role="presentation">
	{#if samples.length > 1 && width}
		<svg {width} height={HEIGHT} role="img" aria-label="{label}, last {minutes} minutes">
			<line class="base" x1={PAD} x2={width - PAD} y1={HEIGHT - PAD} y2={HEIGHT - PAD} />
			<path class="area" d={area} />
			<path class="line" d={line} />
			{#if point}
				{#if hover !== null}<line class="cross" x1={point[0]} x2={point[0]} y1={PAD - 2} y2={HEIGHT - PAD} />{/if}
				<circle cx={point[0]} cy={point[1]} r="4" />
			{/if}
		</svg>
		<div class="caption">
			{#if hover !== null && samples[hover]}
				<strong>{format(samples[hover].value)}</strong>
				<span>{agoText(samples[hover].at)}</span>
			{:else}
				<span>Last {minutes} minutes</span>
				<span>now</span>
			{/if}
		</div>
	{:else}
		<div class="waiting" style:height={`${HEIGHT + 18}px`}>The trend appears after a few updates.</div>
	{/if}
</div>

<style>
	.spark {
		min-width: 0;
	}
	svg {
		display: block;
		overflow: visible;
		cursor: crosshair;
	}
	.base {
		stroke: var(--border);
		stroke-width: 1;
	}
	.area {
		fill: var(--accent);
		opacity: 0.1;
	}
	.line {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.cross {
		stroke: var(--text-muted);
		stroke-width: 1;
	}
	circle {
		fill: var(--accent);
		stroke: var(--surface-1);
		stroke-width: 2;
	}
	.caption {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		height: 18px;
		padding: 2px 5px 0;
		color: var(--text-muted);
		font-size: 11.5px;
	}
	.caption strong {
		color: var(--text);
	}
	.waiting {
		display: flex;
		align-items: center;
		color: var(--text-muted);
		font-size: 11.5px;
	}
</style>

<script lang="ts">
	import type { Level } from '../api';

	let { level, size = 10 }: { level: Level; size?: number } = $props();

	// The shape says the same as the color, like HealthIcon: diamond, triangle, ring.
	const LABELS: Record<Level, string> = { info: 'Info', warning: 'Warning', error: 'Error' };
</script>

<svg class={level} width={size} height={size} viewBox="0 0 10 10" role="img" aria-label={LABELS[level]}>
	<title>{LABELS[level]}</title>
	{#if level === 'error'}
		<path d="M5 0.4 9.6 5 5 9.6 0.4 5Z" />
	{:else if level === 'warning'}
		<path d="M5 0.8 9.4 9H0.6Z" />
	{:else}
		<circle cx="5" cy="5" r="3.6" />
	{/if}
</svg>

<style>
	svg {
		flex: none;
	}
	.error {
		fill: var(--bad);
	}
	.warning {
		fill: var(--warn-mark);
	}
	.info {
		fill: none;
		stroke: var(--text-muted);
		stroke-width: 1.6;
	}
</style>

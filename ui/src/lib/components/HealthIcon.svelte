<script lang="ts">
	import type { Health } from '../api';

	let { health, size = 10 }: { health: Health; size?: number } = $props();

	// The shape says the same as the color, for color-blind readers.
	const LABELS: Record<Health, string> = { ok: 'OK', warning: 'Warning', failed: 'Failed' };
</script>

<svg class={health} width={size} height={size} viewBox="0 0 10 10" role="img" aria-label={LABELS[health]}>
	<title>{LABELS[health]}</title>
	{#if health === 'ok'}
		<circle cx="5" cy="5" r="4" />
	{:else if health === 'warning'}
		<path d="M5 0.8 9.4 9H0.6Z" />
	{:else}
		<path d="M5 0.4 9.6 5 5 9.6 0.4 5Z" />
	{/if}
</svg>

<style>
	svg {
		flex: none;
	}
	.ok {
		fill: var(--ok);
	}
	.warning {
		fill: var(--warn-mark);
	}
	.failed {
		fill: var(--bad);
	}
</style>

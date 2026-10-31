<script lang="ts">
	let { values, label }: { values: number[]; label: string } = $props();

	const W = 44;
	const H = 14;
	const top = $derived(Math.max(...values, 1));
	const d = $derived(
		values
			.map((v, i) => `${i ? 'L' : 'M'}${((i / (values.length - 1)) * W).toFixed(1)},${(H - 1 - (v / top) * (H - 2)).toFixed(1)}`)
			.join('')
	);
</script>

{#if values.length >= 3}
	<svg class="spark" width={W} height={H} viewBox="0 0 {W} {H}" role="img" aria-label="{label}, last {values.length} readings">
		<path {d} fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round" />
	</svg>
{/if}

<style>
	.spark {
		display: inline-block;
		vertical-align: middle;
		margin-right: 8px;
		color: var(--text-muted);
		opacity: 0.8;
	}
</style>

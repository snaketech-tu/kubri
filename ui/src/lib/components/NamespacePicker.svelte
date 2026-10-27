<script lang="ts">
	import { ALL_NAMESPACES, app } from '../state.svelte';

	// Keep the current and default namespace selectable even if listing namespaces is forbidden.
	const options = $derived(
		[...new Set([...app.namespaces, app.namespace, app.cluster?.defaultNamespace ?? ''])]
			.filter(Boolean)
			.sort()
	);
</script>

<label class="picker" title="Namespace (:ns, 0 for all)">
	<span>ns</span>
	<select bind:value={app.namespace} disabled={!app.selected?.namespaced}>
		<option value={ALL_NAMESPACES}>All namespaces</option>
		{#each options as ns (ns)}
			<option value={ns}>{ns}</option>
		{/each}
	</select>
</label>

<style>
	.picker {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
		font-size: 13px;
	}
	select {
		min-width: 180px;
	}
</style>

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
	<span class="label">Namespace</span>
	<select bind:value={app.namespace} disabled={app.page === 'resources' && !app.selected?.namespaced}>
		<option value={ALL_NAMESPACES}>All namespaces</option>
		{#each options as ns (ns)}
			<option value={ns}>{ns}</option>
		{/each}
	</select>
</label>

<style>
	.picker {
		display: flex;
		flex-direction: column;
		padding: 5px 8px;
		border-radius: 8px;
	}
	.picker:hover {
		background: var(--surface-2);
	}
	.picker:focus-within {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.label {
		font-size: 10.5px;
		color: var(--text-muted);
	}
	select {
		min-width: 120px;
		max-width: 200px;
		padding: 0 4px 0 0;
		border: none;
		background: none;
		font-weight: 600;
	}
	select:focus {
		box-shadow: none;
	}
</style>

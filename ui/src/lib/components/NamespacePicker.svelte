<script lang="ts">
	import { ALL_NAMESPACES, app } from '../state.svelte';
	import { plainOptions } from '../select';
	import Select from './Select.svelte';

	// Keep the current and default namespace selectable even if listing namespaces is forbidden.
	const options = $derived(
		[...new Set([...app.namespaces, app.namespace, app.cluster?.defaultNamespace ?? ''])]
			.filter(Boolean)
			.sort()
	);
</script>

<label class="picker" title="Namespace (:ns, 0 for all)">
	<span class="label">Namespace</span>
	<Select
		bind:value={app.namespace}
		options={[{ value: ALL_NAMESPACES, label: 'All namespaces' }, ...plainOptions(options)]}
		disabled={app.page === 'resources' && !app.selected?.namespaced}
		quiet
	/>
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
	.picker :global(.select) {
		min-width: 120px;
		max-width: 200px;
		padding: 0 2px 0 0;
		font-weight: 600;
	}
	.picker :global(.select:focus-visible) {
		box-shadow: none;
	}
</style>

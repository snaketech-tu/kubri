<script lang="ts">
	import { ALL_NAMESPACES, app } from '../state.svelte';
	import { LiveList } from '../watch.svelte';

	const namespaces = new LiveList();

	$effect(() => {
		if (!app.cluster) return;
		return namespaces.start({ group: '', version: 'v1', plural: 'namespaces' }, null);
	});
</script>

<label class="picker" title="Namespace">
	<span>ns</span>
	<select bind:value={app.namespace} disabled={!app.selected?.namespaced}>
		<option value={ALL_NAMESPACES}>All namespaces</option>
		{#each namespaces.items as ns (ns.metadata.name)}
			<option value={ns.metadata.name}>{ns.metadata.name}</option>
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

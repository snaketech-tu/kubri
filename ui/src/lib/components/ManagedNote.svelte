<script lang="ts">
	import type { Manager, ObjectRef } from '../api';
	import { managerNote, managerOf } from '../gitops';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';

	let { target, compact = false }: { target: ObjectRef; compact?: boolean } = $props();

	let manager = $state<Manager | null>(null);

	$effect(() => {
		const t = target;
		let stale = false;
		manager = null;
		managerOf(t).then((m) => !stale && (manager = m));
		return () => (stale = true);
	});

	function open() {
		if (!manager) return;
		const r = manager.owner.resource;
		const resource = app.resources.find((x) => x.group === r.group && x.version === r.version && x.plural === r.plural);
		if (resource) app.showDetails(resource, manager.owner);
	}
</script>

{#if manager}
	<div class="managed" class:reverts={manager.reverts} class:compact role="note">
		<Icon name={manager.reverts ? 'warning' : 'pending'} size={14} />
		<span>
			{managerNote(manager)}
			{#if !compact}<button class="link" onclick={open}>Open {manager.kind}</button>{/if}
		</span>
	</div>
{/if}

<style>
	.managed {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		margin: 12px 20px 0;
		padding: 8px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		font-size: 12.5px;
		line-height: 1.5;
	}
	.managed.compact {
		margin: 0 0 12px;
	}
	.managed.reverts {
		border-color: color-mix(in srgb, var(--warn) 35%, var(--border));
		background: color-mix(in srgb, var(--warn) 8%, var(--surface-1));
	}
	.managed :global(svg) {
		flex: none;
		margin-top: 2px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}
</style>

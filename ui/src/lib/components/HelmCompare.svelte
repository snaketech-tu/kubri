<script lang="ts">
	import { manifestChanges, valuesDiffer, type ManifestChangeKind } from '../helm';
	import DiffView from './DiffView.svelte';

	let {
		namespace,
		before,
		after,
		valuesLabel = 'Given values'
	}: {
		/** The release namespace, where objects without one land. */
		namespace: string;
		before: { values: unknown; manifest: string };
		after: { values: unknown; manifest: string };
		valuesLabel?: string;
	} = $props();

	const changes = $derived(manifestChanges(before.manifest, after.manifest, namespace));
	const counts = $derived(
		changes.reduce(
			(n, c) => ({ ...n, [c.change]: n[c.change] + 1 }),
			{ changed: 0, added: 0, removed: 0, unchanged: 0 } as Record<ManifestChangeKind, number>
		)
	);
	const differ = $derived(valuesDiffer(before.values, after.values));
	const LABEL: Record<ManifestChangeKind, string> = {
		changed: 'Changes',
		added: 'New',
		removed: 'Deleted',
		unchanged: 'Same'
	};
</script>

<div class="compare">
	<h4>{valuesLabel}</h4>
	{#if differ}
		<DiffView before={before.values ?? {}} after={after.values ?? {}} changed="lines of values" maxHeight="280px" />
	{:else}
		<p class="muted">The same.</p>
	{/if}

	<h4>
		Objects
		<span class="muted counts">
			{counts.changed} changed · {counts.added} new · {counts.removed} deleted · {counts.unchanged} the same
		</span>
	</h4>
	{#each changes.filter((c) => c.change !== 'unchanged') as c (`${c.change}/${c.kind}/${c.namespace}/${c.name}`)}
		<details open={c.change === 'changed'}>
			<summary>
				<span class="badge {c.change}">{LABEL[c.change]}</span>
				<span class="kind">{c.kind}</span>
				<span class="mono">{c.name}</span>
				{#if c.namespace && c.namespace !== namespace}<span class="muted">in {c.namespace}</span>{/if}
			</summary>
			<DiffView
				before={c.before}
				after={c.after}
				changed="lines in the rendered manifest"
				unchanged="No changes."
				maxHeight="420px"
			/>
		</details>
	{:else}
		<p class="muted">No object changes.</p>
	{/each}
</div>

<style>
	.compare {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	h4 {
		display: flex;
		align-items: baseline;
		gap: 10px;
		margin: 12px 0 2px;
		font-size: 12px;
		font-weight: 650;
	}
	.counts {
		font-weight: 400;
	}
	details {
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		padding: 6px 10px;
	}
	details[open] {
		padding-bottom: 10px;
	}
	details[open] summary {
		margin-bottom: 8px;
	}
	summary {
		display: flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-size: 13px;
	}
	.badge {
		padding: 1px 7px;
		border-radius: 5px;
		font-size: 11px;
		font-weight: 650;
		background: var(--surface-2);
	}
	.badge.added {
		background: color-mix(in srgb, var(--ok) 16%, transparent);
	}
	.badge.removed {
		background: color-mix(in srgb, var(--bad) 16%, transparent);
	}
	.badge.changed {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
	}
	.kind {
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		margin: 0;
		color: var(--text-muted);
	}
</style>

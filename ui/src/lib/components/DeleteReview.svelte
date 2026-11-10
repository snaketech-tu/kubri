<script lang="ts">
	import { backend, type DeleteOptions, type ObjectRef, type Reference, type ResourceInfo } from '../api';
	import { targetName, usedByText } from '../action-dialog';
	import { t } from '../i18n';
	import { hasUsers, isPods } from '../pods';
	import Select from './Select.svelte';

	let {
		resource,
		targets,
		statuses,
		force = $bindable(),
		propagation = $bindable()
	}: {
		resource: ResourceInfo;
		targets: ObjectRef[];
		/** A word on each target's state, such as Completed, by `namespace/name`. */
		statuses?: Record<string, string>;
		force: boolean;
		propagation: NonNullable<DeleteOptions['propagation']>;
	} = $props();

	/** What uses the objects about to be deleted, which breaks without them; null while looking. */
	let users = $state<{ target: string; refs: Reference[] }[] | null>(null);
	$effect(() => {
		if (!hasUsers(resource)) return;
		const looked = targets.slice(0, 20);
		let current = true;
		Promise.all(
			looked.map((t) =>
				backend
					.usedBy(t)
					.then((refs) => ({ target: targetName(t), refs }))
					// Not knowing is no reason to block a delete.
					.catch(() => ({ target: targetName(t), refs: [] as Reference[] }))
			)
		).then((found) => current && (users = found.filter((u) => u.refs.length)));
		return () => (current = false);
	});
</script>

<p>
	{targets.length === 1 ? t('This deletes') : t('This deletes {n} objects:', { n: targets.length })}
</p>
<ul class="targets">
	{#each targets as t (targetName(t))}
		<li>
			{targetName(t)}{#if statuses?.[targetName(t)]}<span class="muted"> · {statuses[targetName(t)]}</span>{/if}
		</li>
	{/each}
</ul>
{#if hasUsers(resource)}
	{#if users === null}
		<p class="muted">{targets.length === 1 ? t('Looking for what uses it…') : t('Looking for what uses them…')}</p>
	{:else if users.length}
		<div class="blast" role="alert">
			<strong>{t('Still in use.')}</strong>
			{#each users as u (u.target)}
				<p>{usedByText(u, targets.length === 1)}</p>
				<ul>
					{#each u.refs.slice(0, 8) as r (r.kind + r.namespace + r.name)}
						<li>{r.kind} <span class="mono">{r.name}</span>{r.how ? ` (${r.how})` : ''}</li>
					{/each}
					{#if u.refs.length > 8}<li class="muted">{t('and {n} more', { n: u.refs.length - 8 })}</li>{/if}
				</ul>
			{/each}
		</div>
	{/if}
{/if}
<label class="row">
	<input type="checkbox" bind:checked={force} />
	{isPods(resource) ? t('Force: skip the grace period; the container may still be running on its node') : t('Force: skip the grace period')}
</label>
<label class="row">
	{t('Objects it owns')}
	<Select
		bind:value={propagation}
		options={[
			{ value: 'background', label: t('delete them too (recommended)') },
			{ value: 'foreground', label: t('delete them first, then this one') },
			{ value: 'orphan', label: t('keep them') }
		]}
	/>
</label>

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	/* Every target is listed, to review before deleting; long lists scroll. */
	.targets {
		max-height: 220px;
		margin: 0 0 14px;
		padding-left: 18px;
		overflow: auto;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
	}
	.blast {
		margin: 0 0 14px;
		padding: 10px 12px;
		border: 1px solid color-mix(in srgb, var(--bad) 35%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		font-size: 13px;
	}
	.blast strong {
		color: var(--bad);
	}
	.blast p {
		margin: 4px 0;
	}
	.blast ul {
		margin: 0 0 4px;
		padding-left: 18px;
	}
</style>

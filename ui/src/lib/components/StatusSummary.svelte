<script lang="ts">
	import type { KubeObject, ObjectRef } from '../api';
	import { certificateStatus } from '../certs';
	import { relative } from '../format';
	import { gitopsStatus, type ManagedObject } from '../gitops';
	import { app } from '../state.svelte';
	import HealthIcon from './HealthIcon.svelte';

	let { object, target, now }: { object: KubeObject; target: ObjectRef; now: number } = $props();

	const status = $derived(gitopsStatus(target.resource, object) ?? certificateStatus(target.resource, object, now));
	/** The managed objects that need a look first. */
	const managed = $derived(
		[...(status?.managed ?? [])].sort((a, b) => rank(b) - rank(a) || a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name))
	);
	const SHOWN = 50;
	let all = $state(false);

	function rank(m: ManagedObject) {
		return (m.health && m.health !== 'Healthy' ? 2 : 0) + (m.sync && m.sync !== 'Synced' ? 1 : 0);
	}

	function open(m: ManagedObject) {
		const kinds = app.resources.filter((r) => r.group === m.group && r.kind === m.kind);
		const resource = kinds.find((r) => r.version === m.version) ?? kinds[0];
		if (!resource) {
			app.toast(`The cluster does not serve ${m.kind}`, 'bad');
			return;
		}
		const ref = { group: resource.group, version: resource.version, plural: resource.plural };
		app.showDetails(resource, { context: target.context, resource: ref, namespace: m.namespace, name: m.name });
	}
</script>

{#if status}
	<div class="summary" aria-label="Status">
		<section class="card {status.health ?? ''}">
			<h3>
				{#if status.health}<HealthIcon health={status.health} size={11} />{/if}
				{status.state}
			</h3>
			{#if status.message}<p class="message">{status.message}</p>{/if}
			{#if status.facts.length}
				<dl>
					{#each status.facts as fact (fact.label)}
						<dt>{fact.label}</dt>
						<dd title={fact.time ? fact.value : undefined}>{fact.time ? relative(fact.value, now) : fact.value}</dd>
					{/each}
				</dl>
			{/if}
			{#if managed.length}
				<h4>Manages {managed.length} {managed.length === 1 ? 'object' : 'objects'}</h4>
				<table>
					<tbody>
						{#each all ? managed : managed.slice(0, SHOWN) as m (`${m.group}/${m.kind}/${m.namespace}/${m.name}`)}
							<tr>
								<td class="kind">{m.kind}</td>
								<td><button class="link" onclick={() => open(m)}>{m.namespace ? `${m.namespace}/` : ''}{m.name}</button></td>
								{#if m.sync !== undefined || m.health !== undefined}
									<td class:off={m.sync && m.sync !== 'Synced'}>{m.sync ?? ''}</td>
									<td class:off={m.health && m.health !== 'Healthy'} title={m.message}>{m.health ?? ''}</td>
								{/if}
							</tr>
						{/each}
					</tbody>
				</table>
				{#if managed.length > SHOWN && !all}
					<button class="link more" onclick={() => (all = true)}>Show all {managed.length}</button>
				{/if}
			{/if}
		</section>
	</div>
{/if}

<style>
	.summary {
		padding: 16px 20px 0;
	}
	.card {
		padding: 14px 16px;
		border: 1px solid var(--border);
		border-radius: 9px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.card.failed {
		border-color: color-mix(in srgb, var(--bad) 35%, var(--border));
		background: color-mix(in srgb, var(--bad) 7%, var(--surface-1));
	}
	.card.warning {
		border-color: color-mix(in srgb, var(--warn) 35%, var(--border));
		background: color-mix(in srgb, var(--warn) 8%, var(--surface-1));
	}
	h3 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: 13.5px;
	}
	h4 {
		margin: 14px 0 4px;
		font-size: 12.5px;
	}
	.message {
		margin: 8px 0 0;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 3px 14px;
		margin: 10px 0 0;
	}
	dt {
		color: var(--text-muted);
	}
	dd {
		margin: 0;
		min-width: 0;
		font-family: var(--mono);
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	td {
		padding: 2px 10px 2px 0;
		vertical-align: baseline;
	}
	td.kind {
		width: 1%;
		color: var(--text-muted);
		white-space: nowrap;
	}
	td.off {
		color: var(--warn);
		font-weight: 600;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font-family: var(--mono);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
		overflow-wrap: anywhere;
	}
	.link:hover {
		text-decoration: underline;
	}
	.more {
		margin-top: 6px;
		font-family: inherit;
	}
</style>

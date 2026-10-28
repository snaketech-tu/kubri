<script lang="ts">
	import type { Rule } from '../api';
	import { allows, matrix, VERBS } from '../rbac';

	let { rules }: { rules: Rule[] } = $props();

	const rows = $derived(matrix(rules));
</script>

{#if rows.length}
	<div class="wrap">
		<table>
			<thead>
				<tr>
					<th class="resource">Resource</th>
					<th>Group</th>
					{#each VERBS as verb (verb)}<th class="verb" title={verb}>{verb === 'deletecollection' ? 'del-all' : verb}</th>{/each}
					<th>Other</th>
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.key)}
					<tr>
						<td class="resource mono" class:wild={row.resource === '*'}>
							{row.resource}
							{#if row.names.length}<span class="names" title={row.names.join(', ')}>only {row.names.join(', ')}</span>{/if}
						</td>
						<td class="mono muted" class:wild={row.group === '*'}>{row.url ? 'URL' : row.group}</td>
						{#each VERBS as verb (verb)}
							{@const yes = allows(row, verb)}
							<td class="verb" class:yes aria-label="{verb}: {yes ? 'allowed' : 'not allowed'}">{yes ? '✓' : '·'}</td>
						{/each}
						<td class="mono" class:wild={row.verbs.has('*')}>{row.verbs.has('*') ? 'all verbs' : row.extras.join(', ')}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{:else}
	<p class="muted">No rules.</p>
{/if}

<style>
	.wrap {
		overflow-x: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12.5px;
	}
	th {
		padding: 5px 8px;
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
		white-space: nowrap;
	}
	td {
		padding: 4px 8px;
		border-bottom: 1px solid var(--border-subtle);
		vertical-align: top;
	}
	.resource {
		min-width: 140px;
	}
	.verb {
		width: 1%;
		text-align: center;
		color: var(--text-muted);
	}
	td.verb.yes {
		color: var(--ok);
		font-weight: 700;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.wild {
		color: var(--warn);
		font-weight: 600;
	}
	.names {
		display: block;
		max-width: 260px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-muted);
		font-family: Inter, system-ui, sans-serif;
		font-size: 11px;
		font-weight: 400;
	}
</style>

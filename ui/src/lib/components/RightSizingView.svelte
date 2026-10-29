<script lang="ts">
	import { backend, type RightSizing, type SizeAdvice } from '../api';
	import { cpu, errorMessage, memory } from '../format';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';

	let found = $state<RightSizing | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);
	const namespace = app.namespace || null;

	async function load() {
		loading = true;
		try {
			found = await backend.rightSizing(namespace);
			error = null;
		} catch (err) {
			error = errorMessage(err);
		} finally {
			loading = false;
		}
	}
	$effect(() => {
		load();
	});

	const amount = (a: SizeAdvice, value: number) =>
		a.resource === 'cpu' ? (value > 0 && value < 0.0005 ? '<1m' : cpu(value * 1000)) : memory(value);
	/** What the requests free once lowered, per resource. */
	const freed = $derived.by(() => {
		const sum = (resource: string) =>
			(found?.advice ?? []).filter((a) => a.resource === resource && a.reason === 'overRequested').reduce((n, a) => n + a.freed, 0);
		return { cpu: sum('cpu'), memory: sum('memory') };
	});

	function why(a: SizeAdvice): { text: string; tone: 'bad' | 'warn' | '' } {
		const cpuOrMemory = a.resource === 'cpu' ? 'CPU' : 'memory';
		switch (a.reason) {
			case 'nearLimit':
				return a.resource === 'cpu'
					? { text: 'Close to its CPU limit, so throttled', tone: 'warn' }
					: { text: 'Close to its memory limit, so at risk of being OOM-killed', tone: 'bad' };
			case 'underRequested':
				return { text: `Uses more ${cpuOrMemory} than it requests`, tone: 'warn' };
			case 'overRequested': {
				const times = a.used > 0 && a.requested ? Math.round(a.requested / a.used) : 0;
				return { text: times >= 2 ? `Requests ${times}× what it uses` : 'Requests far more than it uses', tone: '' };
			}
			case 'noRequest':
				return { text: `Requests no ${cpuOrMemory}, so the scheduler does not count it`, tone: '' };
		}
	}

	function open(a: SizeAdvice) {
		const resource = findResource(app.resources, a.workload.resource);
		if (resource) app.showDetails(resource, a.workload, 'metrics');
	}

	function review(a: SizeAdvice) {
		const resource = findResource(app.resources, a.workload.resource);
		if (!resource) return;
		app.dialog = {
			kind: 'resources',
			resource,
			target: a.workload,
			container: a.container,
			name: a.resource,
			request: a.suggestedRequest ?? a.request ?? '',
			limit: a.suggestedLimit ?? a.limit ?? '',
			current: { request: a.request, limit: a.limit },
			note: `${why(a).text}: ${a.resource === 'cpu' ? 'at the 95th percentile' : 'at its peak'} it used ${amount(a, a.used)}${found?.history ? ' over the last week' : ' now'}.`
		};
	}
</script>

<section class="page">
	<PageHeader
		crumbs={['Explore', 'Right-sizing']}
		title="Right-sizing"
		count={found?.advice.length}
		subtitle="Containers that request much more or less than they use, or run close to their limit{namespace ? `, in ${namespace}` : ''}"
	>
		{#snippet tools()}
			<button class="btn" onclick={load} disabled={loading}>{loading ? 'Measuring…' : 'Measure again'}</button>
		{/snippet}
	</PageHeader>

	{#if error}
		<div class="banner">{error}</div>
	{:else if !found}
		<p class="muted">Measuring what containers use…</p>
	{:else}
		<p class="source">
			Use from {found.source}{found.history
				? ': CPU at the 95th percentile, memory at its peak.'
				: '. Without Prometheus this is one sample, so Kubri only calls a request too big at four times what is used now.'}
			{#if freed.cpu > 0 || freed.memory > 0}
				Lowering the requests below frees <strong>{[freed.cpu > 0 ? `${cpu(freed.cpu * 1000)} CPU` : '', freed.memory > 0 ? memory(freed.memory) : ''].filter(Boolean).join(' and ')}</strong>
				for other pods.
			{/if}
		</p>
		<div class="card">
			{#if found.advice.length}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>Workload</th>
								<th>Container</th>
								<th>Why</th>
								<th class="numeric">Request</th>
								<th class="numeric">Limit</th>
								<th class="numeric">Used</th>
								<th class="numeric">Suggested</th>
								<th></th>
							</tr>
						</thead>
						<tbody>
							{#each found.advice as a (`${a.workload.namespace}/${a.workload.name}/${a.container}/${a.resource}`)}
								{@const reason = why(a)}
								<tr>
									<td>
										<button class="name" onclick={() => open(a)} title="Show its metrics">{a.workload.name}</button>
										<div class="muted small">{a.kind}{namespace ? '' : ` · ${a.workload.namespace}`}</div>
									</td>
									<td class="mono">{a.container}</td>
									<td>
										<div class="why {reason.tone}">
											{#if reason.tone}<Icon name={reason.tone === 'bad' ? 'critical' : 'warning'} size={14} />{/if}
											<span><strong>{a.resource === 'cpu' ? 'CPU' : 'Memory'}</strong> · {reason.text}</span>
										</div>
									</td>
									<td class="numeric mono">{a.request ?? '—'}</td>
									<td class="numeric mono">{a.limit ?? '—'}</td>
									<td class="numeric mono">{amount(a, a.used)}</td>
									<td class="numeric mono suggested">
										{#if a.suggestedRequest}<span title="Request">{a.suggestedRequest}</span>{/if}
										{#if a.suggestedLimit}<span title="Limit">limit {a.suggestedLimit}</span>{/if}
									</td>
									<td class="action">
										{#if !app.readOnly}<button class="btn" onclick={() => review(a)}>Review…</button>{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="empty"><Icon name="ok" size={16} /> Every container requests about what it uses.</p>
			{/if}
		</div>
	{/if}
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.source {
		margin: 0 0 12px;
		color: var(--text-muted);
		font-size: 12.5px;
		line-height: 1.5;
	}
	.source strong {
		color: var(--text);
	}
	.card {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.table-wrap {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 9px 10px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	th:first-child,
	td:first-child {
		padding-left: 16px;
	}
	td {
		padding: 9px 10px;
		border-bottom: 1px solid var(--border-subtle);
		vertical-align: top;
	}
	.numeric {
		text-align: right;
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.small {
		font-size: 11.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.name {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-family: var(--mono);
		font-size: 12.5px;
		text-align: left;
		cursor: pointer;
	}
	.name:hover {
		text-decoration: underline;
	}
	.why {
		display: flex;
		align-items: flex-start;
		gap: 6px;
		min-width: 240px;
	}
	.why :global(svg) {
		flex: none;
		margin-top: 2px;
	}
	.why.bad :global(svg) {
		color: var(--bad);
	}
	.why.warn :global(svg) {
		color: var(--warn);
	}
	.suggested {
		color: var(--text);
		font-weight: 600;
	}
	.suggested span {
		display: block;
	}
	.action {
		text-align: right;
	}
	.empty {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		padding: 18px 16px;
		color: var(--text-muted);
	}
	.banner {
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, var(--surface-1));
		color: var(--bad);
		font-size: 12.5px;
	}
</style>

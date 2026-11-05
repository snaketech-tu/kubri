<script lang="ts">
	import { plural, t } from '../i18n';
	import { coreText } from '../i18n/errors';
	import { backend, type RightSizing, type SizeAdvice } from '../api';
	import { cpu, errorMessage, memory } from '../format';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';
	import Phrase from './Phrase.svelte';

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
		const isCpu = a.resource === 'cpu';
		switch (a.reason) {
			case 'nearLimit':
				return isCpu
					? { text: t('Close to its CPU limit, so throttled'), tone: 'warn' }
					: { text: t('Close to its memory limit, so at risk of being OOM-killed'), tone: 'bad' };
			case 'underRequested':
				return { text: isCpu ? t('Uses more CPU than it requests') : t('Uses more memory than it requests'), tone: 'warn' };
			case 'overRequested': {
				const times = a.used > 0 && a.requested ? Math.round(a.requested / a.used) : 0;
				return { text: times >= 2 ? t('Requests {times}× what it uses', { times }) : t('Requests far more than it uses'), tone: '' };
			}
			case 'noRequest':
				return {
					text: isCpu ? t('Requests no CPU, so the scheduler does not count it') : t('Requests no memory, so the scheduler does not count it'),
					tone: ''
				};
		}
	}

	/** How much the container used, and how that was measured. */
	function usedNote(a: SizeAdvice) {
		const used = amount(a, a.used);
		if (!found?.history) return t('it uses {amount} now.', { amount: used });
		return a.resource === 'cpu'
			? t('at the 95th percentile it used {amount} over the last week.', { amount: used })
			: t('at its peak it used {amount} over the last week.', { amount: used });
	}

	function open(a: SizeAdvice) {
		const resource = findResource(app.resources, a.workload.resource);
		if (resource) app.showDetails(resource, a.workload, 'metrics');
	}

	function review(a: SizeAdvice) {
		const resource = findResource(app.resources, a.workload.resource);
		if (!resource) return;
		// One review for the whole workload: its other advice is filled in too.
		const same = (x: SizeAdvice) => x.workload.namespace === a.workload.namespace && x.workload.name === a.workload.name && x.kind === a.kind;
		const others = (found?.advice ?? []).filter((x) => x !== a && same(x) && (x.suggestedRequest || x.suggestedLimit));
		app.dialog = {
			kind: 'resources',
			resource,
			target: a.workload,
			fill: [a, ...others].map((x) => ({ container: x.container, name: x.resource, request: x.suggestedRequest, limit: x.suggestedLimit })),
			note: others.length
				? `${why(a).text}: ${usedNote(a)} ${plural(others.length, 'The suggestion for {n} more container of it is filled in too.', 'The suggestions for {n} more containers of it are filled in too.')}`
				: `${why(a).text}: ${usedNote(a)}`
		};
	}
</script>

<section class="page">
	<PageHeader
		crumbs={[t('Right-sizing')]}
		title={t('Right-sizing')}
		count={found?.advice.length}
		subtitle={namespace
			? t('Containers that request much more or less than they use, or run close to their limit, in {namespace}', { namespace })
			: t('Containers that request much more or less than they use, or run close to their limit')}
	>
		{#snippet tools()}
			<button class="btn" onclick={load} disabled={loading}>{loading ? t('Measuring…') : t('Measure again')}</button>
		{/snippet}
	</PageHeader>

	{#if error}
		<div class="banner">{error}</div>
	{:else if !found}
		<p class="muted">{t('Measuring what containers use…')}</p>
	{:else}
		<p class="source">
			{found.history
				? t('Use from {source}: CPU at the 95th percentile, memory at its peak.', { source: coreText(found.source) })
				: t('Use from {source}. Without Prometheus this is one sample, so Kubri only calls a request too big at four times what is used now.', { source: coreText(found.source) })}
			{#if freed.cpu > 0 || freed.memory > 0}
				<Phrase
					text={t('Lowering the requests below frees {amount} for other pods.')}
					strong={{ amount: [freed.cpu > 0 ? `${cpu(freed.cpu * 1000)} CPU` : '', freed.memory > 0 ? memory(freed.memory) : ''].filter(Boolean).join(t(' and ')) }}
				/>
			{/if}
		</p>
		<div class="card">
			{#if found.advice.length}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>{t('Workload')}</th>
								<th>{t('Container')}</th>
								<th>{t('Why')}</th>
								<th class="numeric">{t('Request')}</th>
								<th class="numeric">{t('Limit')}</th>
								<th class="numeric">{t('Used')}</th>
								<th class="numeric">{t('Suggested')}</th>
								<th></th>
							</tr>
						</thead>
						<tbody>
							{#each found.advice as a (`${a.workload.namespace}/${a.workload.name}/${a.container}/${a.resource}`)}
								{@const reason = why(a)}
								<tr>
									<td>
										<button class="name" onclick={() => open(a)} title={t('Show its metrics')}>{a.workload.name}</button>
										<div class="muted small">{a.kind}{namespace ? '' : ` · ${a.workload.namespace}`}</div>
									</td>
									<td class="mono">{a.container}</td>
									<td>
										<div class="why {reason.tone}">
											{#if reason.tone}<Icon name={reason.tone === 'bad' ? 'critical' : 'warning'} size={14} />{/if}
											<span><strong>{a.resource === 'cpu' ? 'CPU' : t('Memory')}</strong> · {reason.text}</span>
										</div>
									</td>
									<td class="numeric mono">{a.request ?? '—'}</td>
									<td class="numeric mono">{a.limit ?? '—'}</td>
									<td class="numeric mono">{amount(a, a.used)}</td>
									<td class="numeric mono suggested">
										{#if a.suggestedRequest}<span title={t('Request')}>{a.suggestedRequest}</span>{/if}
										{#if a.suggestedLimit}<span title={t('Limit')}>{t('limit {suggestedLimit}', { suggestedLimit: a.suggestedLimit })}</span>{/if}
									</td>
									<td class="action">
										{#if !app.readOnly}<button class="btn" onclick={() => review(a)}>{t('Review…')}</button>{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="empty"><Icon name="ok" size={16} /> {t('Every container requests about what it uses.')}</p>
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

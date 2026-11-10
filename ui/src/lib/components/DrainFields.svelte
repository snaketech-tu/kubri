<script lang="ts">
	import type { DrainReport, ObjectRef } from '../api';
	import { plural, t } from '../i18n';
	import Phrase from './Phrase.svelte';

	let {
		target,
		report,
		ignoreDaemonsets = $bindable(),
		deleteEmptydirData = $bindable(),
		force = $bindable(),
		gracePeriod = $bindable()
	}: {
		target: ObjectRef;
		/** What the drain did, once it ran. */
		report: DrainReport | null;
		ignoreDaemonsets: boolean;
		deleteEmptydirData: boolean;
		force: boolean;
		gracePeriod: number | null;
	} = $props();

	function pods(n: number) {
		return plural(n, '{n} pod', '{n} pods');
	}
</script>

{#if report}
	{#if report.failed.length && !report.evicted.length}
		<p>
			<Phrase
				text={report.failed.length === 1
					? t('{name} is cordoned, but no pods were evicted because this pod blocks the drain:')
					: t('{name} is cordoned, but no pods were evicted because these pods block the drain:')}
				strong={{ name: target.name }}
			/>
		</p>
	{:else}
		<p>
			<Phrase text={t('Evicted {pods} from {name}.', { pods: pods(report.evicted.length) })} strong={{ name: target.name }} />
			{report.failed.length ? t('These could not be evicted:') : ''}
		</p>
	{/if}
	{#if report.failed.length}
		<ul class="issues bad">
			{#each report.failed as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
		</ul>
	{/if}
	{#if report.skipped.length}
		<details>
			<summary class="muted">{plural(report.skipped.length, '{n} pod left running', '{n} pods left running')}</summary>
			<ul class="issues">
				{#each report.skipped as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
			</ul>
		</details>
	{/if}
{:else}
	<p>
		<Phrase text={t('Cordon {name} and evict its pods. PodDisruptionBudgets are respected.')} strong={{ name: target.name }} />
	</p>
	<label class="row"><input type="checkbox" bind:checked={ignoreDaemonsets} /> {t('Ignore DaemonSet pods')}</label>
	<label class="row"><input type="checkbox" bind:checked={deleteEmptydirData} /> {t('Delete emptyDir data')}</label>
	<label class="row">
		<input type="checkbox" bind:checked={force} /> {t('Force: also evict pods no controller will recreate')}
	</label>
	<label class="row">
		{t('Grace period')}
		<input class="grace" type="number" min="0" bind:value={gracePeriod} placeholder={t('pod default')} />
		{t('seconds')}
	</label>
{/if}

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
	}
	.grace {
		width: 130px;
	}
	.issues {
		margin: 6px 0 10px;
		padding-left: 18px;
		font-size: 13px;
	}
	.issues.bad {
		color: var(--bad);
	}
	.issues code {
		font-family: var(--mono);
		font-size: 12px;
		color: var(--text);
	}
</style>

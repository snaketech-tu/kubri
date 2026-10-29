<script lang="ts" module>
	/** The range last picked, kept while Kubri runs: comparing objects needs the same range. */
	let lastRange = 3600;
</script>

<script lang="ts">
	import { backend, type Charts, type ObjectRef } from '../api';
	import { RANGES } from '../charts';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';
	import DashboardList from './DashboardList.svelte';
	import TimeChart from './TimeChart.svelte';

	let { target }: { target: ObjectRef } = $props();

	/** Charts ending now follow along. */
	const REFRESH = 30_000;

	let seconds = $state(lastRange);
	let charts = $state<Charts | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);
	let request = 0;

	async function load() {
		const id = ++request;
		loading = true;
		try {
			const next = await backend.charts(target, { seconds });
			if (id !== request) return;
			charts = next;
			error = null;
		} catch (err) {
			if (id !== request) return;
			error = errorMessage(err);
		} finally {
			if (id === request) loading = false;
		}
	}

	const prometheus = $derived(app.has('prometheus'));

	$effect(() => {
		void target;
		void seconds;
		lastRange = seconds;
		if (!prometheus) return;
		load();
		const timer = setInterval(load, REFRESH);
		return () => clearInterval(timer);
	});

	function copy(query: string) {
		navigator.clipboard.writeText(query).then(
			() => app.toast('Copied the PromQL query'),
			(err) => app.toast(errorMessage(err), 'bad')
		);
	}

	const time = (at: number) =>
		new Date(at * 1000).toLocaleString(undefined, { weekday: seconds > 86400 ? 'short' : undefined, hour: '2-digit', minute: '2-digit' });
</script>

<div class="metrics">
	{#if app.has('grafana')}
		<DashboardList {target} />
	{/if}
	{#if prometheus}
		<div class="bar">
			<div class="ranges" role="group" aria-label="Time range">
				{#each RANGES as range (range.seconds)}
					<button class:active={seconds === range.seconds} aria-pressed={seconds === range.seconds} onclick={() => (seconds = range.seconds)}>
						{range.label}
					</button>
				{/each}
			</div>
			<span class="source">
				{#if loading && !charts}Asking Prometheus…{:else if charts}From Prometheus at {charts.source}{/if}
			</span>
		</div>

		{#if error}
			<div class="banner">{error}</div>
		{/if}

		{#if charts}
			{#if !charts.charts.length}
				<p class="muted">Prometheus has no data for {target.name} in this range. It may be newer than that, or Prometheus may not collect cAdvisor metrics.</p>
			{/if}
			<div class="grid">
				{#each charts.charts as chart (chart.id)}
					<section>
						<TimeChart {chart} start={charts.start} end={charts.end} step={charts.step} markers={charts.markers} />
						<button class="link" onclick={() => copy(chart.query)} title={chart.query}>Copy PromQL</button>
					</section>
				{/each}
			</div>
			{#if charts.markers.length}
				<div class="markers">
					<strong>In this range</strong>
					<ul>
						{#each charts.markers as marker (marker.at + marker.label)}
							<li><span class="at">{time(marker.at)}</span> {marker.label}</li>
						{/each}
					</ul>
				</div>
			{/if}
			{#if charts.missing.length}
				<p class="muted note">
					No data for {charts.missing.join(', ').toLowerCase()}.
					{#if charts.missing.includes('Container restarts')}Restarts come from kube-state-metrics.{/if}
				</p>
			{/if}
		{/if}
	{/if}
</div>

<style>
	.metrics {
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 14px 20px 20px;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.ranges {
		display: flex;
		border: 1px solid var(--border);
		border-radius: 7px;
		overflow: hidden;
	}
	.ranges button {
		padding: 3px 11px;
		border: none;
		border-right: 1px solid var(--border);
		background: var(--surface-1);
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.ranges button:last-child {
		border-right: none;
	}
	.ranges button.active {
		background: var(--surface-2);
		color: var(--text);
		font-weight: 600;
	}
	.source {
		color: var(--text-muted);
		font-size: 12px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
		gap: 22px 28px;
	}
	section {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.link {
		align-self: flex-end;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 11.5px;
	}
	.markers {
		font-size: 12.5px;
	}
	.markers ul {
		margin: 4px 0 0;
		padding: 0;
		list-style: none;
	}
	.markers .at {
		display: inline-block;
		min-width: 90px;
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}
	.muted {
		margin: 0;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.banner {
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, var(--surface-1));
		color: var(--bad);
		font-size: 12.5px;
	}
</style>

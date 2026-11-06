<script lang="ts">
	import { t } from '../i18n';
	import { backend, type ObjectRef, type Usage } from '../api';
	import { reading, type Measure } from '../limits';

	/** What a pod uses right now, against its requests and limits, from metrics-server. */
	let { target }: { target: ObjectRef } = $props();

	const REFRESH_MS = 15_000;
	let usage = $state<Usage | null>(null);
	let missing = $state(false);

	$effect(() => {
		const { namespace, name } = target;
		let stopped = false;
		usage = null;
		missing = false;
		const load = async () => {
			try {
				const list = await backend.metrics('pods', namespace ?? null);
				if (stopped) return;
				usage = list.find((u) => u.name === name) ?? null;
				missing = !usage;
			} catch {
				if (!stopped) missing = true;
			}
		};
		load();
		const timer = setInterval(() => document.hidden || load(), REFRESH_MS);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	});

	const rows = $derived(
		usage
			? ([
					['cpu', t('CPU')],
					['memory', t('Memory')]
				] as [Measure, string][]).map(([what, label]) => ({ what, label, r: reading(usage!, what) }))
			: []
	);
</script>

{#if usage}
	<section class="glance" aria-label={t('Use right now')}>
		<h3>{t('Use right now')}</h3>
		{#each rows as { what, label, r } (what)}
			<div class="row">
				<span class="label">{label}</span>
				<span class="value">
					<strong class:bad={r.strained}>{r.used}</strong>
					{#if r.limit}<span class="muted">{t('of {limit} limit', { limit: r.limit })}</span>{/if}
					{#if r.request}<span class="muted">· {t('requests {request}', { request: r.request })}</span>{/if}
					{#if !r.limit && !r.request}<span class="muted">{t('no request or limit set')}</span>{/if}
					{#if r.strained}
						<span class="strain">{what === 'cpu' ? t('at its limit, so throttled') : t('close to its limit, may be stopped')}</span>
					{/if}
				</span>
				{#if r.share !== null}
					<span
						class="meter"
						role="meter"
						aria-valuemin="0"
						aria-valuemax="100"
						aria-valuenow={Math.round(r.share * 100)}
						aria-label={r.against === 'limit' ? t('{label} of its limit', { label }) : t('{label} of its request', { label })}
						title={r.against === 'limit' ? t('Against the limit') : t('Against the request: no limit is set')}
					>
						<span class="fill" class:full={r.share >= 0.9} style:width="{Math.round(r.share * 100)}%"></span>
					</span>
				{/if}
			</div>
		{/each}
	</section>
{:else if missing}
	<p class="muted none">{t('Use is not measured: metrics-server does not report this pod.')}</p>
{/if}

<style>
	.glance {
		display: grid;
		gap: 8px;
		margin: 16px 20px 0;
		padding: 12px 14px;
		border: 1px solid var(--border);
		border-radius: 9px;
		font-size: 12.5px;
	}
	h3 {
		margin: 0;
		font-size: 13.5px;
	}
	.row {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr);
		gap: 2px 10px;
		align-items: baseline;
	}
	.label {
		color: var(--text-muted);
	}
	.value {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 6px;
		align-items: baseline;
	}
	strong.bad {
		color: var(--bad);
	}
	.muted {
		color: var(--text-muted);
	}
	.none {
		margin: 12px 20px 0;
		font-size: 12px;
	}
	.strain {
		padding: 0 6px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 14%, transparent);
		color: var(--bad);
		font-size: 11.5px;
		font-weight: 600;
	}
	.meter {
		grid-column: 2;
		position: relative;
		height: 5px;
		border-radius: 3px;
		background: var(--surface-2);
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 3px;
		background: var(--accent);
	}
	.fill.full {
		background: var(--bad);
	}
</style>

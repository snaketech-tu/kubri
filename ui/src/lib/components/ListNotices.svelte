<script lang="ts">
	import { age } from '../format';
	import { t } from '../i18n';
	import { coreText } from '../i18n/errors';
	import type { LiveMetrics } from '../watch.svelte';
	import Icon from './Icon.svelte';

	let {
		problem,
		error,
		notice,
		metrics,
		now,
		onretry
	}: {
		/** Why the list may be wrong or empty. */
		problem: string | null;
		/** What the cluster answered, after the problem. */
		error: string | null;
		/** What the core says about the list, such as a column it could not read. */
		notice: string | null;
		/** The list's metrics, for kinds that have them. */
		metrics: LiveMetrics | null;
		now: number;
		onretry: () => void;
	} = $props();
</script>

{#if problem}
	<div class="banner" role="alert">
		<Icon name="critical" />
		<span><strong>{problem}</strong> {error}</span>
		<button onclick={onretry}>{t('Retry')}</button>
	</div>
{/if}
{#if notice}
	<div class="metrics-note" role="status">{coreText(notice)}.</div>
{/if}
{#if metrics?.error}
	<div class="metrics-note" role="status">
		{metrics.available ? t('Metrics are stale') : t('Metrics are unavailable')}{metrics.updated
			? t(', last measured {age} ago', { age: age(new Date(metrics.updated).toISOString(), now) })
			: ''}: {coreText(metrics.error)}
	</div>
{/if}

<style>
	.banner {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		margin-bottom: 12px;
		padding: 10px 14px;
		border: 1px solid color-mix(in srgb, var(--bad) 30%, var(--border));
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, var(--surface-1));
		color: var(--bad);
		font-size: 13px;
	}
	.banner span {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.banner button {
		padding: 5px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-weight: 600;
	}
	.metrics-note {
		margin-bottom: 12px;
		font-size: 12px;
		color: var(--text-muted);
	}
</style>

<script lang="ts">
	import { t } from '../i18n';
	import { backend, type Dashboards, type ObjectRef } from '../api';
	import { errorMessage } from '../format';
	import { copyDashboard, openDashboard } from '../integrations';
	import Icon from './Icon.svelte';

	let {
		target,
		title = 'Grafana',
		empty = t('Grafana has no dashboard for this object.')
	}: { target: ObjectRef; title?: string; empty?: string } = $props();

	let found = $state<Dashboards | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		const current = target;
		let cancelled = false;
		found = null;
		error = null;
		backend.dashboards(current).then(
			(d) => !cancelled && (found = d),
			(err) => !cancelled && (error = errorMessage(err))
		);
		return () => {
			cancelled = true;
		};
	});
</script>

<div class="dashboards">
	<strong>{title}</strong>
	{#if error}
		<p class="muted">{error}</p>
	{:else if !found}
		<p class="muted">{t('Looking for dashboards…')}</p>
	{:else if !found.links.length}
		<p class="muted">{empty}{found.note ? ` Grafana's own list: ${found.note}` : ''}</p>
	{:else}
		{#if found.note}<p class="muted">{t('Kubri could not read Grafana\'s dashboards: {note}', { note: found.note })}</p>{/if}
		<ul>
			{#each found.links as link, i (`${i}:${link.path}`)}
				<li>
					<button
						class="link"
						onclick={() => openDashboard(link)}
						title={link.filled.length ? `Open in Grafana with ${link.filled.join(', ')} set` : t('Open in Grafana')}
					>
						<Icon name="popout" size={13} />
						{link.title}
					</button>
					{#if link.folder}<span class="muted">{link.folder}</span>{/if}
					<button class="copy" onclick={() => copyDashboard(link)} aria-label={t('Copy the address of {title}', { title: link.title })}>{t('Copy')}</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.dashboards {
		font-size: 12.5px;
	}
	ul {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin: 4px 0 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		text-align: left;
	}
	.muted {
		margin: 4px 0 0;
		color: var(--text-muted);
	}
	.copy {
		margin-left: auto;
		padding: 0 6px;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--surface-1);
		color: var(--text-muted);
		font-size: 11px;
		flex: none;
	}
	.copy:hover {
		color: var(--text);
	}
	li .muted {
		margin: 0;
		font-size: 11.5px;
	}
</style>

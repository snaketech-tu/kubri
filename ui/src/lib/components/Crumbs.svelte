<script lang="ts" module>
	/** A breadcrumb step; one with `onclick` leads to that place. */
	export type Crumb = string | { label: string; onclick: () => void; title?: string };
</script>

<script lang="ts">
	import { ALL_NAMESPACES, app } from '../state.svelte';
	import { t } from '../i18n';
	import Icon from './Icon.svelte';

	let {
		crumbs,
		prefix = 'namespace'
	}: {
		crumbs: Crumb[];
		/**
		 * Where the page sits: the context, and its namespace unless the page is not namespaced
		 * (`context`) or shows cluster-wide objects (`cluster`); none for pages across contexts.
		 */
		prefix?: 'namespace' | 'cluster' | 'context' | 'none';
	} = $props();

	/** Every page's path starts in the context, as the header shows it. */
	const trail = $derived.by((): Crumb[] => {
		if (prefix === 'none' || !app.cluster) return crumbs;
		const label = app.prefs.contextName(app.cluster.context);
		// On the overview the context leads to the other contexts; elsewhere to its overview.
		const start: Crumb[] = [
			app.page === 'pulses'
				? { label, onclick: () => (app.contextMenuOpen = true), title: `${t('Switch context')} (Ctrl+Shift+C)` }
				: { label, onclick: () => app.showPulses(), title: t('Overview') }
		];
		if (prefix === 'namespace') {
			start.push({
				label: app.namespace === ALL_NAMESPACES ? t('all namespaces') : app.namespace,
				onclick: () => app.namespacePickRequest++,
				title: t('Namespace (:ns, 0 for all)')
			});
		} else if (prefix === 'cluster') start.push(t('cluster-wide'));
		return [...start, ...crumbs];
	});
</script>

<nav class="crumbs" aria-label={t('Breadcrumb')}>
	{#each trail as crumb, i (i)}
		{#if i}<Icon name="next" size={12} />{/if}
		{#if typeof crumb === 'string'}
			<span aria-current={i === trail.length - 1 ? 'page' : undefined}>{crumb}</span>
		{:else}
			<button onclick={crumb.onclick} title={crumb.title}>{crumb.label}</button>
		{/if}
	{/each}
</nav>

<style>
	.crumbs {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.crumbs button {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font-size: inherit;
	}
	.crumbs button:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
</style>

<script lang="ts">
	import { t } from '../i18n';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';

	/*
	 * Says "Production" beside the context or namespace that is, instead of a banner that would read
	 * like an error. The red stripe over the header goes with it; what it means is in the tooltip.
	 */
	let { what }: { what: 'context' | 'namespace' } = $props();

	const title = $derived(
		`${what === 'context' ? t('Production') : t('Production namespace')} · ${app.readOnly ? t('changes are off') : t('changes go live; deleting asks you to type the name')}`
	);
</script>

<span class="production-badge" {title} role="note" aria-label={title}>
	<Icon name="shield" size={12} />
	{t('Production')}
</span>

<style>
	.production-badge {
		display: inline-flex;
		flex: none;
		align-items: center;
		gap: 4px;
		padding: 1px 8px 1px 6px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 13%, var(--surface-1));
		color: var(--bad);
		font-size: 11.5px;
		font-weight: 600;
		line-height: 1.5;
		white-space: nowrap;
	}
</style>

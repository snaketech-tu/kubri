<script lang="ts">
	import { ALL_NAMESPACES, app } from '../state.svelte';
	import { t } from '../i18n';
	import Select from './Select.svelte';
	import ProductionBadge from './ProductionBadge.svelte';
	import type { OptionToggle } from '../select';

	// Keep the current and default namespace selectable even if listing namespaces is forbidden.
	const options = $derived(
		[...new Set([...app.namespaces, app.namespace, app.cluster?.defaultNamespace ?? ''])]
			.filter(Boolean)
			.sort()
	);

	/** The shield beside each namespace, in a context that is not production as a whole. */
	function productionToggle(ns: string): OptionToggle | undefined {
		if (!app.cluster || app.production || app.cluster.snapshot) return undefined;
		const rule = app.productionRuleOf(ns);
		if (rule && rule.by !== 'name') {
			const label = t('Production by the rule {rule}. Change it in the context settings.', { rule: rule.rule });
			return { label, on: true, run: () => app.toast(label) };
		}
		return {
			label: rule ? t('Production. Click to unmark.') : t('Mark {name} as production', { name: ns }),
			on: !!rule,
			run: () => void app.setProductionNamespace(ns, !rule)
		};
	}
</script>

<label class="picker" title={t('Namespace (:ns, 0 for all)')}>
	<span class="label">{t('Namespace')}</span>
	<Select
		bind:value={app.namespace}
		options={[
			{ value: ALL_NAMESPACES, label: t('All namespaces') },
			...options.map((ns) => ({ value: ns, label: ns, toggle: productionToggle(ns) }))
		]}
		disabled={app.page === 'resources' && !app.selected?.namespaced}
		searchable
		quiet
	/>
</label>
{#if app.cluster && app.productionNamespace}<ProductionBadge what="namespace" />{/if}

<style>
	.picker {
		display: flex;
		flex-direction: column;
		padding: 5px 8px;
		border-radius: 8px;
	}
	.picker:hover {
		background: var(--surface-2);
	}
	.picker:focus-within {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.label {
		font-size: 10.5px;
		color: var(--text-muted);
	}
	.picker :global(.select) {
		min-width: 120px;
		max-width: 200px;
		padding: 0 2px 0 0;
		font-weight: 600;
	}
	.picker :global(.select:focus-visible) {
		box-shadow: none;
	}
</style>

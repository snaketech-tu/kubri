<script lang="ts">
	import { plural, t } from '../i18n';
	import { app } from '../state.svelte';
	import { withMarked, type Suggestion } from '../production';
	import Icon from './Icon.svelte';

	/*
	 * Namespaces that look like production by their name or an environment label. Kubri never marks
	 * them itself: the user marks them one by one, or keeps the rules that found them.
	 */
	let { context, suggestions, ondismiss }: { context: string; suggestions: Suggestion[]; ondismiss?: () => void } = $props();

	const rules = $derived([...new Set(suggestions.map((s) => s.rule))]);

	function markAll() {
		void app.setProductionNamespace(
			suggestions.map((s) => s.name),
			true
		);
	}

	async function keepRules() {
		const settings = app.prefs.contextSettingsOf(context);
		const patterns = suggestions.filter((s) => s.by === 'pattern').map((s) => s.rule);
		const labels = suggestions.filter((s) => s.by === 'labels').map((s) => s.rule);
		const ok = await app.prefs.setContextSettings(context, {
			...settings,
			productionNamespaces: withMarked(settings, patterns, true),
			productionLabels: [...new Set([...(settings.productionLabels ?? []), ...labels])]
		});
		if (ok) app.toast(plural(rules.length, 'Added {n} rule for production namespaces', 'Added {n} rules for production namespaces'));
	}
</script>

<div class="hint" role="note">
	<Icon name="shield" size={16} />
	<div class="text">
		<strong>{t('These look like production')}</strong>
		<span>
			{#each suggestions as s, i (s.name)}<code>{s.name}</code>&nbsp;<small>({s.by === 'labels' ? s.rule : t('name')})</small>{i < suggestions.length - 1 ? ', ' : ''}{/each}
		</span>
	</div>
	<div class="buttons">
		{#if context === app.cluster?.context}
			<button class="btn primary" onclick={markAll}>{t('Mark {n} as production', { n: suggestions.length })}</button>
		{/if}
		<button class="btn" onclick={keepRules} title={rules.join(', ')}>{plural(rules.length, 'Add the rule {rule}', 'Add {n} rules', { rule: rules[0] })}</button>
		{#if ondismiss}<button class="btn quiet" onclick={ondismiss}>{t('Do not ask again')}</button>{/if}
	</div>
</div>

<style>
	.hint {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 8px 10px;
		padding: 9px 12px;
		border: 1px solid color-mix(in srgb, var(--warn) 35%, var(--border));
		border-radius: 8px;
		background: color-mix(in srgb, var(--warn) 9%, var(--surface-1));
		color: var(--text);
		font-size: 12.5px;
	}
	.hint > :global(.icon) {
		flex: none;
		margin-top: 1px;
		color: var(--warn);
	}
	.text {
		display: grid;
		flex: 1 1 220px;
		gap: 2px;
		min-width: 0;
	}
	.text span {
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	code {
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
	}
	small {
		font-size: 11.5px;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.buttons .primary:hover {
		border-color: var(--accent-strong);
	}
	.buttons .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.buttons .quiet {
		border-color: transparent;
		background: none;
		color: var(--text-muted);
	}
</style>

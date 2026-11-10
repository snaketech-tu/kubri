<script lang="ts">
	import { t } from '../i18n';
	import type { ContextSettings } from '../api';
	import { app } from '../state.svelte';
	import { markedNames, namePatterns, productionRule, suggestions, tidyLabelRule, withMarked } from '../production';
	import Icon from './Icon.svelte';
	import ProductionSuggestions from './ProductionSuggestions.svelte';

	/*
	 * Which namespaces of a context get production's care: picked from the namespaces Kubri sees,
	 * or by rules, a name pattern like `*-prod` or a label like `env=production`, which also cover
	 * namespaces made later. The preview shows what the rules match now.
	 */
	let { context, namespaces = [] }: { context: string; namespaces?: string[] } = $props();

	const id = $props.id();
	const current = $derived(app.prefs.contextSettingsOf(context));
	/** Labels are known for the connected context only. */
	const labels = $derived(context === app.cluster?.context ? app.namespaceLabels : {});
	const names = $derived(markedNames(current));
	const patterns = $derived(namePatterns(current));
	const labelRules = $derived(current.productionLabels ?? []);
	/** Names marked one by one that the list does not show, e.g. ones the user may not list. */
	const unlisted = $derived(names.filter((n) => !namespaces.includes(n)));
	const matched = $derived(namespaces.filter((n) => productionRule(current, n, labels[n])));
	const offered = $derived(suggestions(current, namespaces.map((name) => ({ name, labels: labels[name] }))));

	let filter = $state('');
	let draft = $state('');
	let problem = $state('');
	const listed = $derived(filter ? namespaces.filter((n) => n.includes(filter.trim())) : namespaces);

	function save(change: Partial<ContextSettings>) {
		return app.prefs.setContextSettings(context, { ...current, ...change });
	}

	function mark(name: string, on: boolean) {
		void save({ productionNamespaces: withMarked(current, [name], on) });
	}

	/** Adds what was typed: a label rule when it has `=` or starts with `!`, else a name or pattern. */
	function add() {
		const value = draft.trim();
		problem = '';
		if (!value) return;
		if (/[=!]/.test(value)) {
			const rule = tidyLabelRule(value);
			if (!rule) {
				problem = t('Label rules look like env=production, env!=dev, team or !canary');
				return;
			}
			if (!labelRules.includes(rule)) void save({ productionLabels: [...labelRules, rule] });
		} else {
			if (!/^[a-z0-9*][a-z0-9*.-]*$/.test(value)) {
				problem = t('Add a name like shop, a pattern like *-prod or a label rule like env=production');
				return;
			}
			void save({ productionNamespaces: withMarked(current, [value], true) });
		}
		draft = '';
	}

	function remove(rule: string, labelRule: boolean) {
		if (labelRule) void save({ productionLabels: labelRules.filter((r) => r !== rule) });
		else void save({ productionNamespaces: withMarked(current, [rule], false) });
	}
</script>

<div class="production" role="group" aria-labelledby="{id}-title">
	<span class="title" id="{id}-title">{t('Production namespaces')}</span>

	{#if namespaces.length}
		<div class="pick">
			{#if namespaces.length > 8}
				<input class="filter" bind:value={filter} placeholder={t('Filter namespaces')} aria-label={t('Filter namespaces')} spellcheck="false" />
			{/if}
			<ul aria-label={t('Namespaces')}>
				{#each listed as ns (ns)}
					{@const rule = productionRule(current, ns, labels[ns])}
					{@const byRule = !!rule && rule.by !== 'name'}
					<li>
						<label class:on={!!rule}>
							<input type="checkbox" checked={!!rule} disabled={byRule} onchange={(e) => mark(ns, e.currentTarget.checked)} />
							<span class="name">{ns}</span>
							{#if byRule}<span class="via" title={t('Production by the rule {rule}', { rule: rule!.rule })}>{rule!.rule}</span>{/if}
						</label>
					</li>
				{:else}
					<li class="none">{t('Nothing matches')}</li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="rules">
		{#if patterns.length || labelRules.length || unlisted.length}
			<ul class="chips" aria-label={t('Rules')}>
				{#each unlisted as name (name)}
					<li class="chip"><span>{name}</span><button aria-label={t('Remove {rule}', { rule: name })} onclick={() => remove(name, false)}><Icon name="close" size={12} /></button></li>
				{/each}
				{#each patterns as pattern (pattern)}
					<li class="chip" title={t('Name pattern: * stands for any text')}><span>{pattern}</span><button aria-label={t('Remove {rule}', { rule: pattern })} onclick={() => remove(pattern, false)}><Icon name="close" size={12} /></button></li>
				{/each}
				{#each labelRules as rule (rule)}
					<li class="chip" title={t('Label rule')}><small>{t('label')}</small><span>{rule}</span><button aria-label={t('Remove {rule}', { rule })} onclick={() => remove(rule, true)}><Icon name="close" size={12} /></button></li>
				{/each}
			</ul>
		{/if}
		<form
			class="add"
			onsubmit={(e) => {
				e.preventDefault();
				add();
			}}
		>
			<input
				bind:value={draft}
				oninput={() => (problem = '')}
				placeholder={namespaces.length ? t('Add a rule, e.g. *-prod or env=production') : t('Add a namespace or rule, e.g. shop, *-prod or env=production')}
				aria-label={t('Add a production namespace or rule')}
				aria-invalid={!!problem}
				spellcheck="false"
			/>
			<button class="btn" type="submit" disabled={!draft.trim()}>{t('Add')}</button>
		</form>
		{#if problem}<p class="problem" role="alert">{problem}</p>{/if}
	</div>

	{#if namespaces.length}
		<p class="now" aria-live="polite">
			{#if matched.length}
				<strong>{t('Production now: {n}', { n: matched.length })}</strong>
				<span class="list">{matched.join(', ')}</span>
			{:else}
				{t('No namespace is production yet.')}
			{/if}
		</p>
	{/if}

	{#if offered.length}
		<ProductionSuggestions {context} suggestions={offered} />
	{/if}
</div>

<style>
	.production,
	.pick,
	.rules {
		grid-template-columns: minmax(0, 1fr);
	}
	.production {
		display: grid;
		gap: 8px;
		min-width: 0;
	}
	.title {
		font-size: 12px;
		color: var(--text-muted);
	}
	.pick {
		display: grid;
		gap: 6px;
		min-width: 0;
	}
	.filter {
		font-size: 12.5px;
	}
	.pick ul {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		max-height: 168px;
		margin: 0;
		padding: 4px;
		overflow-y: auto;
		border: 1px solid var(--border);
		border-radius: 8px;
		list-style: none;
		overscroll-behavior: contain;
	}
	.pick label {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		padding: 3px 6px;
		border-radius: 5px;
		font-size: 12.5px;
		cursor: pointer;
	}
	.pick label:hover {
		background: var(--surface-2);
	}
	.pick label.on .name {
		color: var(--bad);
		font-weight: 600;
	}
	.name {
		flex: 0 0 auto;
		max-width: 70%;
		overflow: hidden;
		font-family: var(--mono);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.via {
		flex: 0 1 auto;
		min-width: 0;
		margin-left: auto;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11.5px;
		white-space: nowrap;
	}
	.none {
		padding: 4px 6px;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.rules {
		display: grid;
		gap: 6px;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 100%;
		padding: 2px 3px 2px 9px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
		font-family: var(--mono);
		font-size: 12px;
	}
	.chip span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.chip small {
		font-family: Inter, system-ui, sans-serif;
		font-size: 10.5px;
		opacity: 0.8;
	}
	.chip button {
		display: flex;
		padding: 2px;
		border: none;
		border-radius: 50%;
		background: none;
		color: inherit;
	}
	.chip button:hover {
		background: color-mix(in srgb, var(--bad) 20%, transparent);
	}
	.add {
		display: flex;
		gap: 6px;
	}
	.add input {
		flex: 1;
		min-width: 0;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.problem {
		margin: 0;
		color: var(--bad);
		font-size: 12px;
	}
	.now {
		display: grid;
		gap: 2px;
		margin: 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	.now strong {
		color: var(--text);
		font-weight: 600;
	}
	.now .list {
		font-family: var(--mono);
		font-size: 11.5px;
		overflow-wrap: anywhere;
	}
</style>

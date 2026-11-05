<script lang="ts">
	import { t } from '../i18n';
	import type { ResourceInfo } from '../api';
	import { forControl } from '../keys';
	import { kindGroups, pluralKind } from '../nav';
	import { app } from '../state.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';
	import Phrase from './Phrase.svelte';

	let filter = $state(app.kindsFilter);
	let input = $state<HTMLInputElement>();
	let list = $state<HTMLElement>();

	const groups = $derived(kindGroups(app.resources, filter));
	const total = $derived(kindGroups(app.resources, '').reduce((n, g) => n + g.kinds.length, 0));
	const shown = $derived(groups.reduce((n, g) => n + g.kinds.length, 0));

	$effect(() => {
		input?.focus();
	});

	const buttons = () => [...(list?.querySelectorAll<HTMLButtonElement>('button.kind') ?? [])];

	function onListKey(e: KeyboardEvent) {
		const all = buttons();
		const at = all.indexOf(document.activeElement as HTMLButtonElement);
		if (at === -1) return;
		const step = e.key === 'ArrowDown' || e.key === 'j' ? 1 : e.key === 'ArrowUp' || e.key === 'k' ? -1 : 0;
		if (!step) return;
		e.preventDefault();
		if (at + step < 0) input?.focus();
		else all[Math.min(at + step, all.length - 1)]?.focus();
	}

	// In the field, Enter opens the first kind and ↓ moves into the list. Elsewhere, as after a click on
	// a hint, the same keys and / work; on a kind its own keys do.
	function onkeydown(e: KeyboardEvent) {
		if (e.defaultPrevented || app.overlayOpen || e.ctrlKey || e.metaKey || e.altKey) return;
		if (e.target === input) {
			if (e.key === 'Enter' && groups[0]) {
				e.preventDefault();
				app.open(groups[0].kinds[0]);
			} else if (e.key === 'ArrowDown') {
				e.preventDefault();
				buttons()[0]?.focus();
			}
			return;
		}
		if (forControl(e) || (e.target as HTMLElement | null)?.closest?.('button.kind')) return;
		if (e.key === '/') {
			e.preventDefault();
			input?.focus();
			input?.select();
		} else if (e.key === 'Enter' && groups[0]) {
			e.preventDefault();
			app.open(groups[0].kinds[0]);
		} else if (e.key === 'j' || e.key === 'ArrowDown') {
			e.preventDefault();
			buttons()[0]?.focus();
		}
	}

	const groupName = (group: string) => group || 'core';
	const label = (r: ResourceInfo) => pluralKind(r.kind, r.plural);
</script>

<svelte:window {onkeydown} />

<section class="page">
	<PageHeader crumbs={[t('All kinds')]} prefix="context" title={t('All kinds')} count={total} subtitle={t('Every kind the cluster serves, by API group. Pick one to open its list.')}>
		{#snippet tools()}
			<SearchField bind:value={filter} bind:input label={t('Find a kind')} placeholder={t('Kind, short name or group…')} />
		{/snippet}
	</PageHeader>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="card" bind:this={list} onkeydown={onListKey}>
		{#each groups as { group, kinds } (group)}
			<h2><span class="group">{groupName(group)}</span> <span class="count">{kinds.length}</span></h2>
			<ul>
				{#each kinds as r (r.group + '/' + r.plural)}
					<li>
						<button class="kind" onclick={() => app.open(r)} title={`${r.kind} · ${r.group ? `${r.group}/` : ''}${r.version}`}>
							<span class="name">{label(r)}</span>
							{#if r.shortNames.length || !r.namespaced}
								<span class="meta">
									{#if r.shortNames.length}<code>{r.shortNames.join(', ')}</code>{/if}
									{#if !r.namespaced}<span>{t('cluster-wide')}</span>{/if}
								</span>
							{/if}
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="none">{total ? t('No kind matches "{filter}".', { filter }) : t('The cluster serves no kinds Kubri can list.')}</p>
		{/each}
	</div>
	{#if filter && shown}
		<p class="hint"><Phrase text={t('{enter} opens {kind}, {down} moves into the list.', { kind: label(groups[0].kinds[0]) })} kbd={{ enter: 'Enter', down: '↓' }} /></p>
	{/if}
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.page :global(.search) {
		width: min(320px, 60vw);
	}
	.card {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 4px 16px 16px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	h2 {
		position: sticky;
		top: 0;
		margin: 0;
		padding: 14px 0 6px;
		background: var(--surface-1);
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 400;
	}
	.group {
		color: var(--text);
		font-family: var(--mono);
		font-weight: 600;
	}
	ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 240px), 1fr));
		gap: 2px 12px;
		align-items: start;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.kind {
		display: grid;
		gap: 1px;
		width: 100%;
		padding: 6px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text);
		font-size: 13px;
		text-align: left;
	}
	.kind:hover,
	.kind:focus-visible {
		background: var(--surface-2);
	}
	.name {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0 10px;
		color: var(--text-muted);
		font-size: 11.5px;
	}
	code {
		font-family: var(--mono);
	}
	.none {
		color: var(--text-muted);
		font-size: 13px;
	}
	.hint {
		margin: 10px 0 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	.hint :global(kbd) {
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
		font-size: 11px;
	}
</style>

<script lang="ts">
	import type { ColumnSettings, ResourceInfo } from '../api';
	import { defaultColumns } from '../columns';
	import { t } from '../i18n';
	import type { DisplayColumn } from '../list-cells';
	import { navPlace } from '../nav';
	import { app, findResource, type ListScope } from '../state.svelte';
	import Crumbs, { type Crumb } from './Crumbs.svelte';
	import ColumnChooser from './ColumnChooser.svelte';
	import Icon from './Icon.svelte';

	let {
		resource,
		scope,
		label,
		count,
		wide = $bindable(),
		widens,
		columns,
		settings,
		settingsKey,
		onexport
	}: {
		resource: ResourceInfo;
		scope: ListScope | null;
		/** What the list is called, as the sidebar names it. */
		label: string;
		/** How many objects the list has, before the filter. */
		count: number;
		/** All columns (w), scrolling sideways when they do not fit. */
		wide: boolean;
		/** Whether All columns would show more than the list does. */
		widens: boolean;
		/** Every column the list can show, for the chooser. */
		columns: DisplayColumn[];
		settings: ColumnSettings;
		/** Which kind the column settings are saved for. */
		settingsKey: string;
		onexport: () => void;
	} = $props();

	let chooserOpen = $state(false);

	/** The object a drill-down came from, as a step back to its list and to the object. */
	const owner = $derived.by(() => {
		const from = scope?.from;
		const resource = from?.object?.resource && findResource(app.resources, from.object.resource);
		return from && resource ? { from, resource, list: navPlace(resource).label } : null;
	});

	const trail = $derived.by((): Crumb[] => {
		if (!scope) return [label];
		if (owner) {
			const { from, resource, list } = owner;
			return [
				{ label: list, onclick: () => app.backToOwner(from), title: t('Back to the {kind} list', { kind: resource.kind }) },
				{ label: from.object.name, onclick: () => app.backToOwner(from, true), title: t('Describe {name}', { name: from.object.name }) },
				label
			];
		}
		return [{ label, onclick: () => (app.scope = null), title: t('Show all {kind} objects', { kind: resource.kind }) }, scope.title];
	});
</script>

<header>
	<Crumbs crumbs={trail} prefix={resource.namespaced ? 'namespace' : 'cluster'} />
	<div class="heading">
		<div class="title">
			<h1 title="{resource.kind} · {resource.group ? `${resource.group}/` : ''}{resource.version}">{label}</h1>
			<span class="count">{count}</span>
			{#if app.cluster}
				{@const pin = app.pins.of(resource)}
				<button
					class="pin"
					class:on={app.pins.has(pin)}
					aria-pressed={app.pins.has(pin)}
					onclick={() => app.pins.toggle(pin)}
					title={app.pins.has(pin) ? t('Unpin from the sidebar') : t('Pin this list to the sidebar')}
					aria-label={app.pins.has(pin) ? t('Unpin from the sidebar') : t('Pin to the sidebar')}
				>
					<Icon name="bookmark" size={16} />
				</button>
			{/if}
			{#if scope}
				<span class="chip">
					{scope.title}
					<button onclick={() => (app.scope = null)} title={t('Show all {kind} objects', { kind: resource.kind })} aria-label={t('Show all')}>×</button>
				</span>
			{/if}
		</div>
		<div class="tools">
			{#if widens}
				<button
					class="toggle"
					class:on={wide}
					aria-pressed={wide}
					onclick={() => (wide = !wide)}
					title={wide ? t('Back to the columns that fit (w)') : t('Every column, scrolling sideways when they do not fit (w)')}
				>
					{t('All columns')}
				</button>
			{/if}
			<div class="chooser">
				<button
					class="toggle"
					class:on={chooserOpen}
					aria-expanded={chooserOpen}
					onclick={() => (chooserOpen = !chooserOpen)}
					title={t('Choose, order and add columns for {kind} lists', { kind: resource.kind })}
				>
					{t('Columns')}
				</button>
				{#if chooserOpen}
					<ColumnChooser
						kind={resource.kind}
						{columns}
						{settings}
						defaults={defaultColumns(app.views.of(resource))}
						{wide}
						onchange={(next) => app.prefs.setColumnSettings(settingsKey, next)}
						onclose={() => (chooserOpen = false)}
					/>
				{/if}
			</div>
			<button class="toggle" onclick={onexport} title={t('Save the list as CSV (Ctrl+S)')}>
				<Icon name="download" size={15} /> CSV
			</button>
		</div>
	</div>
</header>

<style>
	header {
		padding: 20px 0 16px;
	}
	.heading {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 8px 16px;
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 10px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 26px;
		letter-spacing: -0.03em;
	}
	.count {
		font-size: 18px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 4px 1px 9px;
		border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
		border-radius: 999px;
		background: color-mix(in srgb, var(--accent) 10%, transparent);
		color: var(--accent-strong);
		font-size: 12px;
		font-weight: 600;
		white-space: nowrap;
	}
	.chip button {
		padding: 0 4px;
		border: none;
		border-radius: 999px;
		background: none;
		color: inherit;
		font-size: 14px;
		line-height: 1;
	}
	.pin {
		display: inline-flex;
		padding: 4px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
	}
	.pin:hover {
		background: var(--surface-2);
	}
	.pin.on {
		color: var(--accent);
	}
	.pin.on :global(path) {
		fill: currentColor;
	}
	.chip button:hover {
		background: color-mix(in srgb, currentColor 15%, transparent);
	}
	.tools {
		display: flex;
		gap: 8px;
	}
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 12px;
		font-weight: 600;
	}
	.toggle:hover {
		border-color: var(--text-muted);
	}
	.toggle.on {
		border-color: var(--accent);
		color: var(--accent-strong);
	}
	.chooser {
		position: relative;
	}
</style>

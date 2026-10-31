<script lang="ts">
	import { forControl } from '../keys';
	import { tick, untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { backend, type ResourceInfo, type XrayNode } from '../api';
	import { errorMessage } from '../format';
	import type { MenuAction } from '../menu';
	import { openObjectMenu } from '../objects';
	import { app, findResource, refOf } from '../state.svelte';
	import { allIds, initialExpanded, issuesBelow, kindTag, visibleRows, xrayResources, type TreeRow } from '../xray';
	import HealthIcon from './HealthIcon.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';
	import Select from './Select.svelte';

	let { resource }: { resource: ResourceInfo } = $props();

	const REFRESH_MS = 15_000;

	let roots = $state.raw<XrayNode[]>([]);
	let loaded = $state(false);
	let error = $state<string | null>(null);
	let filter = $state('');
	let issuesOnly = $state(false);
	let cursorId = $state<string | null>(null);
	const expanded = new SvelteSet<string>();
	let scroller = $state<HTMLDivElement>();
	let filterInput = $state<HTMLInputElement>();

	const namespace = $derived(app.namespace || null);
	const kinds = $derived(xrayResources(app.resources));

	$effect(() => {
		const ref = { group: resource.group, version: resource.version, plural: resource.plural };
		const ns = namespace;
		let cancelled = false;
		let loading = false;
		// Ids seen before keep the state the user gave them; new ones get the default.
		const known = new Set<string>();
		roots = [];
		loaded = false;
		error = null;
		untrack(() => expanded.clear());
		const load = async () => {
			if (loading) return;
			loading = true;
			try {
				const next = await backend.xray(ref, ns);
				if (cancelled) return;
				for (const id of initialExpanded(next)) if (!known.has(id)) expanded.add(id);
				for (const id of allIds(next)) known.add(id);
				roots = next;
				error = null;
			} catch (err) {
				if (!cancelled) error = errorMessage(err);
			} finally {
				loading = false;
				if (!cancelled) loaded = true;
			}
		};
		load();
		const timer = setInterval(() => document.hidden || load(), REFRESH_MS);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	});

	const rows = $derived(visibleRows(roots, expanded, { text: filter, issuesOnly }));
	const cursor = $derived.by(() => {
		const i = rows.findIndex((r) => r.node.id === cursorId);
		return i === -1 ? 0 : i;
	});
	const current = $derived<TreeRow | undefined>(rows[cursor]);
	const withIssues = $derived(roots.filter((r) => r.health !== 'ok' || issuesBelow(r) > 0).length);

	$effect(() => {
		void cursorId;
		tick().then(() => scroller?.querySelector('.row.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	function toggle(row: TreeRow) {
		if (!row.node.children.length) return;
		if (row.expanded) expanded.delete(row.node.id);
		else expanded.add(row.node.id);
	}

	function openDetails(row: TreeRow | undefined) {
		const node = row?.node;
		if (!node?.resource) return;
		const info = findResource(app.resources, node.resource);
		if (!info) return;
		cursorId = node.id;
		app.showDetails(info, { resource: info, namespace: node.namespace, name: node.name });
	}

	/** The row's menu: the object's actions, and opening or closing the tree below it. */
	function openMenu(e: MouseEvent, row: TreeRow) {
		e.preventDefault();
		const node = row.node;
		cursorId = node.id;
		const info = node.resource && findResource(app.resources, node.resource);
		const tree: MenuAction[] = node.children.length
			? [{ group: 'Tree', label: row.expanded ? 'Collapse' : 'Expand', keys: [], run: () => toggle(row) }]
			: [];
		if (!info) {
			if (tree.length) app.menu = { actions: tree, x: e.clientX, y: e.clientY, hints: false };
			return;
		}
		const target = { resource: refOf(info), namespace: node.namespace, name: node.name };
		openObjectMenu(info, target, e.clientX, e.clientY, { before: () => (cursorId = node.id), first: tree });
	}

	function move(delta: number) {
		if (!rows.length) return;
		cursorId = rows[Math.max(0, Math.min(rows.length - 1, cursor + delta))].node.id;
	}

	function expandAll() {
		for (const id of allIds(roots)) expanded.add(id);
	}

	function switchKind(plural: string) {
		const next = kinds.find((k) => k.plural === plural);
		if (next) app.showXray(next);
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		const el = e.target as HTMLElement;
		if (el === filterInput) {
			if (e.key === 'Escape' || e.key === 'Enter' || e.key === 'ArrowDown') {
				if (e.key === 'Escape') filter = '';
				filterInput?.blur();
				e.preventDefault();
			}
			return;
		}
		if (forControl(e)) return;
		if (e.ctrlKey || e.metaKey || e.altKey) return;
		const row = current;
		const handled = (() => {
			switch (e.key) {
				case 'ArrowDown':
				case 'j':
					move(1);
					return true;
				case 'ArrowUp':
				case 'k':
					move(-1);
					return true;
				case 'g':
					move(-rows.length);
					return true;
				case 'G':
					move(rows.length);
					return true;
				case 'ArrowRight':
				case 'l':
					if (!row) return true;
					if (row.node.children.length && !row.expanded) expanded.add(row.node.id);
					else if (row.expanded) move(1);
					return true;
				case 'ArrowLeft':
				case 'h':
					if (!row) return true;
					if (row.expanded) expanded.delete(row.node.id);
					else if (row.parent) cursorId = row.parent;
					return true;
				case ' ':
					if (row) toggle(row);
					return true;
				case 'Enter':
				case 'd':
					openDetails(row);
					return true;
				case '/':
					filterInput?.focus();
					return true;
				case 'f':
					if (!app.details) return false;
					app.maximized = !app.maximized;
					return true;
				case 'Escape':
					if (app.details) {
						app.details = null;
						app.maximized = false;
					} else if (filter) {
						filter = '';
					}
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}
</script>

<svelte:window {onkeydown} />

<section>
	<PageHeader
		crumbs={['Explore', 'Topology']}
		title="Topology"
		count={loaded ? roots.length : null}
		subtitle="XRay of {resource.kind.toLowerCase()}s and what they own, {namespace ? `in ${namespace}` : 'in all namespaces'}"
	>
		{#if withIssues}<span class="tag bad"><HealthIcon health="failed" />{withIssues} with issues</span>{/if}
		{#snippet tools()}
			<button class="btn" onclick={() => app.showGraph()} title="All objects as one graph, with request paths (:graph)">Graph</button>
			<button class="btn" onclick={expandAll} title="Open every tree">Expand all</button>
			<button class="btn" onclick={() => expanded.clear()} title="Close every tree">Collapse all</button>
		{/snippet}
	</PageHeader>

	{#if error}
		<div class="notice" role="alert">Could not build the tree: {error}</div>
	{/if}

	<div class="card">
		<div class="toolbar">
			<label class="start">
				<span>Start from</span>
				<Select value={resource.plural} options={kinds.map((k) => ({ value: k.plural, label: `${k.kind}s` }))} onchange={switchKind} />
			</label>
			<SearchField bind:value={filter} bind:input={filterInput} label="Filter the trees" placeholder="Filter by name or status" />
		</div>
		<div class="tabs" role="tablist" aria-label="Show">
			<button role="tab" aria-selected={!issuesOnly} onclick={() => (issuesOnly = false)}>
				All <span class="n">{roots.length}</span>
			</button>
			<button role="tab" aria-selected={issuesOnly} onclick={() => (issuesOnly = true)}>
				With issues <span class="n" class:alert={withIssues}>{withIssues}</span>
			</button>
		</div>

		<div class="tree" bind:this={scroller} role="tree" aria-label="XRay of {resource.kind}s">
			{#each rows as row, i (row.node.id)}
				{@const node = row.node}
				{@const hidden = row.expanded ? 0 : issuesBelow(node)}
				<div
					class="row"
					class:cursor={i === cursor}
					class:root={row.depth === 0}
					style:padding-left={`${12 + row.depth * 20}px`}
					role="treeitem"
					aria-selected={i === cursor}
					aria-expanded={node.children.length ? row.expanded : undefined}
					tabindex="-1"
					onclick={() => (cursorId = node.id)}
					ondblclick={() => openDetails(row)}
					oncontextmenu={(e) => openMenu(e, row)}
					onkeydown={() => {}}
				>
					<button
						class="chevron"
						class:open={row.expanded}
						class:leaf={!node.children.length}
						tabindex="-1"
						aria-label={row.expanded ? 'Collapse' : 'Expand'}
						onclick={(e) => {
							e.stopPropagation();
							cursorId = node.id;
							toggle(row);
						}}>›</button
					>
					<HealthIcon health={node.health} />
					<span class="tag">{kindTag(node.kind)}</span>
					{#if node.resource}
						<button class="name link" tabindex="-1" onclick={() => openDetails(row)} title="Describe {node.kind} {node.name}">{node.name}</button>
					{:else}
						<span class="name">{node.name}</span>
					{/if}
					{#if row.depth === 0 && !namespace && node.namespace}<span class="ns">{node.namespace}</span>{/if}
					{#if node.detail}<span class="detail {node.health}" title={node.detail}>{node.detail}</span>{/if}
					{#if hidden}<span class="hidden-issues" title="Problems below this row">{hidden} {hidden === 1 ? 'issue' : 'issues'} inside</span>{/if}
				</div>
			{:else}
				<div class="empty">
					{#if !loaded}
						Building the tree…
					{:else if filter || issuesOnly}
						Nothing matches the filter.
					{:else if !error}
						No {resource.kind}s {namespace ? `in ${namespace}` : 'in the cluster'}.
					{/if}
				</div>
			{/each}
		</div>
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 8px;
		border-radius: 5px;
		font-size: 11.5px;
		font-weight: 600;
		white-space: nowrap;
	}
	.tag.bad {
		background: color-mix(in srgb, var(--bad) 13%, var(--surface-1));
		color: var(--bad);
	}
	.card {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px 14px;
		padding: 14px 16px 6px;
	}
	.toolbar > :global(.search) {
		flex: 1;
		min-width: 200px;
	}
	.start {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.tabs {
		display: flex;
		gap: 20px;
		padding: 0 16px;
		border-bottom: 1px solid var(--border);
	}
	.tabs button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 10px 0 9px;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.tabs button:hover {
		color: var(--text);
	}
	.tabs button[aria-selected='true'] {
		border-bottom-color: var(--accent);
		color: var(--accent-strong);
		font-weight: 650;
	}
	.n {
		padding: 0 6px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.n.alert {
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.tree {
		flex: 1;
		overflow: auto;
		padding: 6px 8px 12px;
		font-size: 13px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 7px;
		min-height: 28px;
		padding-right: 12px;
		border-radius: 6px;
		white-space: nowrap;
		outline: none;
	}
	.row:hover {
		background: color-mix(in srgb, var(--surface-2) 55%, transparent);
	}
	.row.cursor {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.row.root {
		margin-top: 4px;
	}
	.chevron {
		width: 18px;
		padding: 0;
		border: none;
		background: none;
		color: var(--text-muted);
		font-size: 15px;
		line-height: 1;
		transition: transform 0.12s;
	}
	.chevron.open {
		transform: rotate(90deg);
	}
	.chevron.leaf {
		visibility: hidden;
	}
	.tag {
		min-width: 44px;
		padding: 0 6px;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
		text-align: center;
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.root .name {
		font-weight: 600;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		text-align: left;
	}
	.link:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	.ns {
		color: var(--text-muted);
		font-size: 12px;
	}
	.detail {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--text-muted);
		font-size: 12px;
	}
	.detail.warning {
		color: var(--warn);
	}
	.detail.failed {
		color: var(--bad);
		font-weight: 600;
	}
	.hidden-issues {
		padding: 0 7px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 14%, transparent);
		color: var(--bad);
		font-size: 11px;
		font-weight: 600;
	}
	@media (max-width: 720px) {
		section {
			padding: 0 12px 12px;
		}
	}
	.empty {
		padding: 40px 24px;
		text-align: center;
		color: var(--text-muted);
	}
</style>

<script lang="ts">
	import { tick } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { canDelete, canEdit, canRestart, canScale } from '../actions';
	import type { MetricsKind, ObjectRef, ResourceInfo, Row } from '../api';
	import { age, cpu, memory } from '../format';
	import { app, refOf } from '../state.svelte';
	import { cellTone } from '../status';
	import { filterRows, isAgeColumn, sortRows, type SortState } from '../table';
	import { LiveMetrics, LiveTable } from '../watch.svelte';

	let { resource, namespace }: { resource: ResourceInfo; namespace: string } = $props();

	const CPU = -2;
	const MEM = -3;

	const table = new LiveTable();
	const metrics = new LiveMetrics();
	const selection = new SvelteSet<string>();
	let filter = $state('');
	let wide = $state(false);
	let sort = $state<SortState>({ column: -1, descending: false });
	let cursorKey = $state<string | null>(null);
	let now = $state(Date.now());
	let filterInput = $state<HTMLInputElement>();
	let scroller = $state<HTMLDivElement>();

	const ns = $derived(resource.namespaced && namespace ? namespace : null);
	const metricsKind = $derived<MetricsKind | null>(
		resource.group === '' && resource.plural === 'pods'
			? 'pods'
			: resource.group === '' && resource.plural === 'nodes'
				? 'nodes'
				: null
	);

	$effect(() => table.start(resource, ns));
	$effect(() => {
		if (metricsKind) return metrics.start(metricsKind, ns);
	});
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	type DisplayColumn =
		| { kind: 'namespace'; label: string }
		| { kind: 'cell'; label: string; index: number; age: boolean; name: boolean }
		| { kind: 'cpu' | 'mem'; label: string; index: number };

	const columns = $derived.by(() => {
		const out: DisplayColumn[] = [];
		if (resource.namespaced && !ns) out.push({ kind: 'namespace', label: 'Namespace' });
		const cells = table.columns
			.map((c, index) => ({ c, index }))
			.filter(({ c }) => wide || c.priority === 0);
		for (const { c, index } of cells) {
			const isAge = isAgeColumn(c);
			if (isAge && metrics.available) {
				out.push({ kind: 'cpu', label: 'CPU', index: CPU }, { kind: 'mem', label: 'Mem', index: MEM });
			}
			out.push({ kind: 'cell', label: c.name, index, age: isAge, name: c.format === 'name' });
		}
		return out;
	});

	const rows = $derived.by(() => {
		const filtered = filterRows(table.rows, filter);
		if (sort.column === CPU || sort.column === MEM) {
			const value = (r: Row) => {
				const u = metrics.get(r.namespace, r.name);
				return u ? (sort.column === CPU ? u.cpu : u.memory) : -1;
			};
			const sorted = [...filtered].sort((a, b) => value(a) - value(b));
			return sort.descending ? sorted.reverse() : sorted;
		}
		return sortRows(filtered, table.columns, sort);
	});

	// Forget selected rows once they are gone, e.g. after deleting them.
	$effect(() => {
		const keys = new Set(table.rows.map((r) => r.key));
		for (const key of [...selection]) if (!keys.has(key)) selection.delete(key);
	});

	const cursor = $derived.by(() => {
		const i = rows.findIndex((r) => r.key === cursorKey);
		return i === -1 ? 0 : i;
	});
	const current = $derived<Row | undefined>(rows[cursor]);

	$effect(() => {
		void cursorKey;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	function target(row: Row): ObjectRef {
		return { resource: refOf(resource), namespace: row.namespace, name: row.name };
	}

	function targets(): ObjectRef[] {
		const chosen = rows.filter((r) => selection.has(r.key));
		if (chosen.length) return chosen.map(target);
		return current ? [target(current)] : [];
	}

	function move(delta: number) {
		if (!rows.length) return;
		const next = Math.max(0, Math.min(rows.length - 1, cursor + delta));
		cursorKey = rows[next].key;
	}

	function sortBy(index: number) {
		sort = sort.column === index ? { column: index, descending: !sort.descending } : { column: index, descending: false };
	}

	function sortByColumn(match: (name: string, format: string) => boolean) {
		const index = table.columns.findIndex((c) => match(c.name, c.format));
		if (index !== -1) sortBy(index);
	}

	function cellText(row: Row, column: DisplayColumn): string {
		switch (column.kind) {
			case 'namespace':
				return row.namespace ?? '';
			case 'cpu': {
				const u = metrics.get(row.namespace, row.name);
				return u ? cpu(u.cpu) : '';
			}
			case 'mem': {
				const u = metrics.get(row.namespace, row.name);
				return u ? memory(u.memory) : '';
			}
			case 'cell': {
				if (column.age) return age(row.created, now);
				const value = row.cells[column.index];
				return value === null || value === undefined ? '' : String(value);
			}
		}
	}

	function open(row: Row | undefined, tab: 'describe' | 'yaml' = 'describe', editing = false) {
		if (!row) return;
		cursorKey = row.key;
		app.showDetails(resource, target(row), tab, editing);
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		const el = e.target as HTMLElement;
		if (el.closest('input, textarea, select, .cm-editor, [contenteditable="true"]')) {
			if (el === filterInput && ['Escape', 'Enter', 'ArrowDown'].includes(e.key)) {
				if (e.key === 'Escape') filter = '';
				filterInput?.blur();
				e.preventDefault();
			}
			return;
		}
		const ctrl = e.ctrlKey || e.metaKey;
		const handled = (() => {
			if (ctrl) {
				switch (e.key.toLowerCase()) {
					case 'a':
						rows.forEach((r) => selection.add(r.key));
						return true;
					case 'd':
						return remove();
				}
				return false;
			}
			if (e.altKey) return false;
			switch (e.key) {
				case 'ArrowDown':
				case 'j':
					move(1);
					return true;
				case 'ArrowUp':
				case 'k':
					move(-1);
					return true;
				case 'PageDown':
					move(20);
					return true;
				case 'PageUp':
					move(-20);
					return true;
				case 'Home':
				case 'g':
					move(-rows.length);
					return true;
				case 'End':
				case 'G':
					move(rows.length);
					return true;
				case 'Enter':
				case 'd':
					open(current);
					return true;
				case 'y':
					open(current, 'yaml');
					return true;
				case 'e':
					if (!canEdit(resource)) return false;
					open(current, 'yaml', true);
					return true;
				case ' ':
					if (current) {
						if (selection.has(current.key)) selection.delete(current.key);
						else selection.add(current.key);
						move(1);
					}
					return true;
				case 'Delete':
					return remove();
				case 's':
					if (!canScale(resource) || !current) return false;
					app.dialog = { kind: 'scale', resource, target: target(current) };
					return true;
				case 'r':
					if (!canRestart(resource) || !current) return false;
					app.dialog = { kind: 'restart', resource, target: target(current) };
					return true;
				case 'a':
					if (!resource.verbs.includes('patch') || !current) return false;
					app.dialog = { kind: 'labels', resource, target: target(current) };
					return true;
				case 'w':
					wide = !wide;
					return true;
				case '/':
					filterInput?.focus();
					return true;
				case 'N':
					sortByColumn((_, format) => format === 'name');
					return true;
				case 'A':
					sortByColumn((name, format) => isAgeColumn({ name, format } as never));
					return true;
				case '0':
					app.namespace = '';
					return true;
				case 'Escape':
					if (app.details) app.details = null;
					else if (selection.size) selection.clear();
					else if (filter) filter = '';
					else return false;
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}

	function remove() {
		const chosen = targets();
		if (!canDelete(resource) || !chosen.length) return false;
		app.dialog = { kind: 'delete', resource, targets: chosen };
		return true;
	}

	const hiddenColumns = $derived(table.columns.filter((c) => c.priority > 0).length);
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<div class="title">
			<h1>{resource.kind}</h1>
			<span class="count">{rows.length}{filter ? ` / ${table.rows.length}` : ''}</span>
			<code>{resource.group ? `${resource.group}/` : ''}{resource.version}</code>
			{#if selection.size}
				<span class="selected">{selection.size} selected</span>
			{/if}
		</div>
		<div class="tools">
			{#if hiddenColumns}
				<button class="toggle" class:on={wide} onclick={() => (wide = !wide)} title="Wide view (w)">Wide</button>
			{/if}
			<input class="filter" bind:this={filterInput} bind:value={filter} placeholder="Filter…  (/)   -l app=web" />
		</div>
	</header>

	{#if table.error}
		<div class="banner">{table.error}</div>
	{/if}

	<div class="table-wrap" bind:this={scroller}>
		<table>
			<thead>
				<tr>
					<th class="check"></th>
					{#each columns as column (column.label + ('index' in column ? column.index : ''))}
						{@const index = column.kind === 'namespace' ? -1 : column.index}
						<th
							class:numeric={column.kind === 'cpu' || column.kind === 'mem'}
							class:sorted={sort.column === index}
							onclick={() => sortBy(index)}
						>
							{column.label}
							{#if sort.column === index}<span class="arrow">{sort.descending ? '↓' : '↑'}</span>{/if}
						</th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each rows as row, i (row.key)}
					<tr
						class:cursor={i === cursor}
						class:checked={selection.has(row.key)}
						onclick={() => (cursorKey = row.key)}
						ondblclick={() => open(row)}
					>
						<td class="check">
							<input
								type="checkbox"
								tabindex="-1"
								checked={selection.has(row.key)}
								onclick={(e) => {
									e.stopPropagation();
									if (selection.has(row.key)) selection.delete(row.key);
									else selection.add(row.key);
								}}
							/>
						</td>
						{#each columns as column (column.label + ('index' in column ? column.index : ''))}
							{@const text = cellText(row, column)}
							<td
								class="{column.kind === 'cell' ? cellTone(column.label, text) : ''}"
								class:name={column.kind === 'cell' && column.name}
								class:muted={column.kind === 'namespace' || (column.kind === 'cell' && column.age)}
								class:numeric={column.kind === 'cpu' || column.kind === 'mem'}
							>
								{text}
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
		{#if table.loading}
			<p class="empty">Loading…</p>
		{:else if rows.length === 0}
			<p class="empty">
				{filter ? 'Nothing matches the filter.' : `No ${resource.kind} objects${ns ? ` in ${ns}` : ''}.`}
			</p>
		{/if}
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 8px 16px;
		padding: 14px 20px 10px;
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 10px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.count {
		white-space: nowrap;
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.selected {
		font-size: 12px;
		font-weight: 600;
		color: var(--accent-strong);
	}
	code {
		color: var(--text-muted);
		font-size: 12px;
	}
	.tools {
		display: flex;
		flex: 1;
		justify-content: flex-end;
		gap: 8px;
		min-width: 200px;
	}
	.toggle {
		padding: 0 12px;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--surface-1);
		font-size: 13px;
	}
	.toggle.on {
		border-color: var(--accent);
		color: var(--accent-strong);
	}
	.filter {
		flex: 0 1 280px;
		min-width: 140px;
	}
	.banner {
		margin: 0 20px 10px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 15%, transparent);
		color: var(--bad);
	}
	.table-wrap {
		flex: 1;
		overflow: auto;
		padding: 0 12px 16px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 8px 10px;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--text-muted);
		white-space: nowrap;
		cursor: pointer;
		user-select: none;
	}
	th:hover,
	th.sorted {
		color: var(--text);
	}
	.arrow {
		margin-left: 2px;
	}
	td {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		max-width: 420px;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.check {
		width: 28px;
		padding-right: 0;
	}
	.check input {
		opacity: 0;
		margin: 0;
	}
	tr:hover .check input,
	tr.checked .check input {
		opacity: 1;
	}
	tbody tr {
		cursor: default;
	}
	tbody tr:hover {
		background: var(--surface-1);
	}
	tbody tr.checked {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.numeric {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.ok {
		color: var(--ok);
	}
	.warn {
		color: var(--warn);
	}
	.bad {
		color: var(--bad);
	}
	.empty {
		padding: 24px;
		text-align: center;
		color: var(--text-muted);
	}
</style>

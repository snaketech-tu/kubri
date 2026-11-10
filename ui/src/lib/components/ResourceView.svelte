<script lang="ts">
	import { completedPods } from '../cleanup';
	import { forControl } from '../keys';
	import { untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { canDelete, saveFile } from '../actions';
	import { type MetricsKind, type ObjectRef, type ResourceInfo, type Row } from '../api';
	import { age } from '../format';
	import { cellOf, cellText, fitColumns, metricColumns, PRODUCTION, strains, type CellSource, type DisplayColumn } from '../list-cells';
	import { markedActions as actionsForMarked, withCellActions } from '../list-actions';
	import { actionFor, keyOf, type MenuAction } from '../menu';
	import { drill, objectActions, refusedAction, type ActionOptions, type Subject } from '../objects';
	import { isNamespaces, isNodes, isPods, isServices } from '../pods';
	import { suggestions } from '../production';
	import ProductionSuggestions from './ProductionSuggestions.svelte';
	import { app, refOf, type DetailsExtra, type DetailsTab, type ListScope } from '../state.svelte';
	import { filterRows, hasHealth, isAgeColumn, isFaulty, sortRows, toCsv, type SortState } from '../table';
	import { LiveMetrics, LiveTable } from '../watch.svelte';
	import { navPlace } from '../nav';
	import { columnsFor, isShown, kindKey, ordered, pathsOf } from '../columns';
	import ActionBar from './ActionBar.svelte';
	import Icon from './Icon.svelte';
	import ListHeader from './ListHeader.svelte';
	import ListNotices from './ListNotices.svelte';
	import ResourceTable from './ResourceTable.svelte';
	import { midSentence, t } from '../i18n';
	import { roving } from '../roving';
	import { store, stored } from '../storage';

	let {
		resource,
		namespace,
		scope = null
	}: { resource: ResourceInfo; namespace: string; scope?: ListScope | null } = $props();

	const table = new LiveTable();
	const metrics = new LiveMetrics();
	const selection = new SvelteSet<string>();
	let filter = $state('');
	/** Bumped by Retry; the watch then starts again and keeps the rows it had. */
	let retry = $state(0);
	let wide = $state(false);
	/** The width the table has, to leave out the columns that matter least when it is narrow. */
	let room = $state(0);
	/** The mark column's width, where the name sticks when the list scrolls sideways. */
	let checkWidth = $state(0);
	/** Only rows that need a look, like k9s Ctrl+Z. */
	let faults = $state(false);
	/** Only running pods. */
	let runningOnly = $state(false);
	let sort = $state<SortState>({ column: -1, descending: false });
	let sortedByUser = false;
	let cursorKey = $state<string | null>(null);
	/**
	 * Whether the user put the cursor somewhere. Until then the keys still work on the first row,
	 * as in k9s, but the action bar offers nothing for a row nobody chose.
	 */
	let picked = $state(false);
	function pick(key: string) {
		cursorKey = key;
		picked = true;
	}
	/** Where Ctrl+Space marks a range from. */
	let anchorKey = $state<string | null>(null);
	let now = $state(Date.now());
	let filterInput = $state<HTMLInputElement>();
	let list = $state<ReturnType<typeof ResourceTable>>();

	const ns = $derived(resource.namespaced && namespace ? namespace : null);
	const metricsKind = $derived<MetricsKind | null>(
		isPods(resource) ? 'pods' : isNodes(resource) ? 'nodes' : isNamespaces(resource) ? 'namespaces' : null
	);
	const writable = $derived(!app.readOnly);

	/** How this kind's list shows its columns, from the settings. */
	const columnKey = $derived(kindKey(resource));
	const columnSettings = $derived(columnsFor(columnKey, app.prefs.settings.columns, app.views.of(resource)));
	const paths = $derived(pathsOf(columnSettings));

	$effect(() =>
		table.start(
			resource,
			ns,
			paths ? { ...(scope?.scope ?? {}), paths } : (scope?.scope ?? null),
			retry > 0,
			app.cluster && !app.cluster.snapshot ? app.cluster.context : null
		)
	);
	$effect(() => {
		if (metricsKind) return metrics.start(metricsKind, ns);
	});
	/**
	 * The rows wait a moment for the first metrics, so their columns are drawn once, not again
	 * when the numbers come. A slow metrics-server does not hold the list for longer than that.
	 */
	let patience = $state(false);
	$effect(() => {
		void metricsKind;
		void ns;
		patience = false;
		const timer = setTimeout(() => (patience = true), 1000);
		return () => clearTimeout(timer);
	});
	const holding = $derived(!!metricsKind && !metrics.answered && !patience);
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	/** What the cells read: getters, so a cell redraws only when what it shows changes. */
	const cells: CellSource = {
		get resource() {
			return resource;
		},
		get columns() {
			return table.columns;
		},
		get metricsKind() {
			return metricsKind;
		},
		usage: (namespace, name) => metrics.get(namespace, name),
		trail: (namespace, name) => metrics.trail(namespace, name),
		get now() {
			return now;
		},
		get taken() {
			return app.snapshot?.taken ?? null;
		},
		productionRuleOf: (name, labels) => app.productionRuleOf(name, labels),
		get thresholds() {
			return app.k9s?.thresholds ?? null;
		}
	};
	const cell = (row: Row, name: string) => cellOf(table.columns, row, name);

	/** Namespaces show which ones get production's care, in a context that is not production as a whole. */
	const marksProduction = $derived(isNamespaces(resource) && !!app.cluster && !app.cluster.snapshot && !app.production);

	/** Namespaces that look like production and are not marked, until the user says not to ask again. */
	let hintDismissed = $state(false);
	$effect(() => {
		hintDismissed = !!app.cluster && stored(`kubri.productionHint.${app.cluster.context}`) === 'off';
	});
	const productionHints = $derived(
		marksProduction && !hintDismissed ? suggestions(app.contextSettings, table.rows.map((r) => ({ name: r.name, labels: r.labels }))) : []
	);

	/** Every column the list can show, in their own order, before the settings and the wide view. */
	const allColumns = $derived.by(() => {
		const out: DisplayColumn[] = [];
		if (resource.namespaced && !ns) out.push({ kind: 'namespace', label: 'Namespace', index: -1, wideOnly: false });
		for (const [index, c] of table.columns.entries()) {
			const isAge = isAgeColumn(c);
			if (isAge && metricsKind) {
				out.push(...metricColumns(metricsKind).map((m) => ({ kind: 'metric' as const, ...m, wideOnly: m.wide })));
			}
			// A custom column comes back named by its path.
			const custom = c.format === 'jsonpath' ? columnSettings.custom.find((x) => x.path === c.name) : undefined;
			if (isAge && marksProduction) out.push({ kind: 'production', label: 'Production', index: PRODUCTION, wideOnly: false });
			out.push({
				kind: 'cell',
				label: isAge ? 'Age' : (custom?.name ?? c.name),
				index,
				age: isAge,
				name: c.format === 'name',
				custom: c.format === 'jsonpath',
				// Ready says what Up-to-date and Available add for most; All columns has them.
				wideOnly: c.priority > 0 || (resource.kind === 'Deployment' && /^(Up-to-date|Available)$/.test(c.name))
			});
		}
		return ordered(out, columnSettings);
	});
	const columns = $derived(allColumns.filter((c) => isShown(c, columnSettings, wide)));

	/** The rows of this view: what the scope's owner owns, if it names one. */
	const owned = $derived.by(() => {
		const owner = scope?.ownerUid;
		return owner ? table.rows.filter((r) => r.owners.some((o) => o.uid === owner)) : table.rows;
	});
	const statusIndex = $derived(table.columns.findIndex((c) => /^(status|phase)$/i.test(c.name)));
	const isRunning = (r: Row) => statusIndex !== -1 && r.cells[statusIndex] === 'Running';
	/** A namespace with failing pods needs a look, though it is Active. */
	const troubled = (r: Row) => isNamespaces(resource) && !!metrics.get(r.namespace, r.name)?.pods?.failed;
	// Pods at their CPU or memory limit need a look too, though they run.
	const faulty = $derived(owned.filter((r) => isFaulty(table.columns, r) || strains(cells, r).length > 0 || troubled(r)));
	/** Kinds with a state, readiness or health; Secrets or Ingresses have nothing to need attention. */
	const healthy = $derived(hasHealth(table.columns, resource.plural));
	const running = $derived(isPods(resource) ? owned.filter(isRunning) : []);
	/** With Ctrl+Z, only the rows that need a look. */
	const scoped = $derived(faults ? faulty : runningOnly ? running : owned);

	function showTab(tab: 'all' | 'attention' | 'running') {
		faults = tab === 'attention';
		runningOnly = tab === 'running';
	}

	// Events read best newest first, until the user sorts otherwise.
	$effect(() => {
		if (sortedByUser || resource.plural !== 'events') return;
		const index = table.columns.findIndex((c) => c.name === 'Last Seen');
		if (index !== -1 && sort.column !== index) sort = { column: index, descending: false };
	});

	/** The tab counts follow the filter, so they match what each tab would show. */
	const counts = $derived({
		all: filterRows(owned, filter).length,
		attention: filterRows(faulty, filter).length,
		running: filterRows(running, filter).length
	});

	// The footer offers no keys for rows when there are none.
	$effect(() => {
		app.listRows = rows.length;
		return () => (app.listRows = null);
	});

	// The palette offers the actions of the row under the cursor.
	$effect(() => {
		const mine = () => (current ? { label: current.name, actions: actionsFor(current, [current]) } : null);
		app.rowActions = mine;
		return () => {
			if (app.rowActions === mine) app.rowActions = null;
		};
	});

	const place = $derived(navPlace(resource));
	/** Why the list may be wrong or empty: never shown as "no objects". */
	const problem = $derived(
		!table.error
			? null
			: table.hasLoaded
				? t('Connection interrupted. Showing the rows from {age} ago.', { age: age(new Date(table.updated).toISOString(), now) || '0s' })
				: /forbidden|403|unauthorized|permission/i.test(table.error)
					? t('Access denied.')
					: t('Could not load the list.')
	);

	const rows = $derived.by(() => {
		const filtered = filterRows(scoped, filter);
		if (sort.column === PRODUCTION && marksProduction) {
			const marked = (r: Row) => (app.productionRuleOf(r.name, r.labels) ? 0 : 1);
			const sorted = [...filtered].sort((a, b) => marked(a) - marked(b) || a.name.localeCompare(b.name));
			return sort.descending ? sorted.reverse() : sorted;
		}
		const metric = columns.find((c) => c.kind === 'metric' && c.index === sort.column);
		if (metric?.kind === 'metric') {
			const value = (r: Row) => {
				const u = metrics.get(r.namespace, r.name);
				return (u && metric.value(u)) ?? -1;
			};
			const sorted = [...filtered].sort((a, b) => value(a) - value(b) || a.name.localeCompare(b.name));
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

	// The cursor holds on to the row it shows, so clearing a filter or sorting keeps it on that row
	// instead of on whatever comes first now.
	$effect(() => {
		const key = current?.key;
		if (key && key !== untrack(() => cursorKey)) cursorKey = key;
	});

	// The cursor stays in view when it moves, and when a filter or sort moves the rows around it.
	// Rows arriving from the cluster do not count: they must not pull the list away from where the user scrolled.
	$effect(() => {
		void cursorKey;
		void holding;
		void filter;
		void sort.column;
		void sort.descending;
		untrack(() => void list?.revealCursor());
	});

	// Plugins and hotkeys run on the row under the cursor, with its cells as shown.
	$effect(() => {
		const focus = () => ({
			resource: refOf(resource),
			target: current ? target(current) : null,
			release: null,
			columns: current ? columns.map((c) => ({ name: c.label, value: cellText(cells, current, c, true) })) : [],
			filter
		});
		app.focus = focus;
		return () => {
			if (app.focus === focus) app.focus = null;
		};
	});

	// A filter from a command like `:pods app=web`.
	$effect(() => {
		if (app.filterRequest === null) return;
		filter = app.filterRequest;
		app.filterRequest = null;
	});

	// The object to put the cursor on, as after jumping to an owner.
	$effect(() => {
		const wanted = app.cursorRequest;
		if (!wanted || table.loading) return;
		const row = table.rows.find(
			(r) => r.name === wanted.name && (wanted.namespace === null || r.namespace === wanted.namespace)
		);
		if (row) pick(row.key);
		else app.toast(t('{name} is not in the list', { name: wanted.name }), 'bad');
		app.cursorRequest = null;
	});

	function target(row: Row): ObjectRef {
		return { context: app.cluster?.context, resource: refOf(resource), namespace: row.namespace, name: row.name };
	}

	/** The marked rows, or else the one under the cursor. */
	function chosenRows(): Row[] {
		const chosen = rows.filter((r) => selection.has(r.key));
		if (chosen.length) return chosen;
		return current ? [current] : [];
	}

	function move(delta: number) {
		if (!rows.length) return;
		const next = Math.max(0, Math.min(rows.length - 1, cursor + delta));
		pick(rows[next].key);
	}

	function toggleMark(row: Row) {
		if (selection.has(row.key)) selection.delete(row.key);
		else selection.add(row.key);
		anchorKey = row.key;
	}

	/** Marks the rows from the last marked one to the cursor, like k9s Ctrl+Space. */
	function markRange() {
		if (!current) return;
		const from = rows.findIndex((r) => r.key === anchorKey);
		const [a, b] = from === -1 ? [cursor, cursor] : [Math.min(from, cursor), Math.max(from, cursor)];
		for (const row of rows.slice(a, b + 1)) selection.add(row.key);
		anchorKey = current.key;
	}

	/** Sorts by a column, or turns its order around. Counts and usage start with the largest, as in k9s. */
	function sortBy(index: number, largestFirst = false) {
		sortedByUser = true;
		sort = sort.column === index ? { column: index, descending: !sort.descending } : { column: index, descending: largestFirst };
	}

	function sortByColumn(match: (name: string, format: string) => boolean, largestFirst = false) {
		const index = table.columns.findIndex((c) => match(c.name, c.format));
		if (index === -1) return false;
		// A column only the wide view shows, like a pod's IP or node.
		if (table.columns[index].priority > 0) wide = true;
		sortBy(index, largestFirst);
		return true;
	}

	/** Sorts by a metrics-server column, such as `CPU` or `%Mem/R`, when the list shows it. */
	function sortByMetric(label: string) {
		const column = allColumns.find((c) => c.kind === 'metric' && c.label === label);
		if (!column) return false;
		// A column only the wide view shows, like the share of a request.
		if (column.wideOnly && !columns.includes(column)) wide = true;
		sortBy(column.index, true);
		return true;
	}

	const named = (wanted: string) => (name: string) => name.toLowerCase() === wanted;

	function open(row: Row | undefined, tab: DetailsTab = 'describe', extra: DetailsExtra = {}) {
		if (!row) return false;
		pick(row.key);
		app.showDetails(resource, target(row), tab, extra);
		return true;
	}

	/** Enter: what belongs to the row, or else its description. */
	async function enter(row: Row | undefined) {
		if (!row) return;
		pick(row.key);
		if (isServices(resource) && cell(row, 'Selector') !== null && /^(|<none>)$/.test(cell(row, 'Selector') ?? '')) {
			open(row);
			return;
		}
		if (!(await drill(resource, target(row)))) open(row);
	}

	/** Offers the successfully finished pods of the list for deletion, with their status to review. */
	function sanitize() {
		const done = completedPods(table.columns, rows);
		if (!done.length) {
			app.toast(t('No successfully completed pods in this view'));
			return;
		}
		const statuses = Object.fromEntries(
			done.map((r) => [`${r.namespace ? `${r.namespace}/` : ''}${r.name}`, cell(r, 'Status') ?? ''])
		);
		app.dialog = { kind: 'delete', resource, targets: done.map(target), statuses };
	}

	function exportCsv() {
		const header = columns.map((c) => c.label);
		const lines = rows.map((r) => columns.map((c) => cellText(cells, r, c, true)));
		const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15);
		saveFile(`${resource.plural}${ns ? `-${ns}` : ''}-${stamp}.csv`, toCsv(header, lines));
	}

	function subject(row: Row): Subject {
		return {
			resource,
			target: target(row),
			owners: row.owners,
			node: isPods(resource) ? cell(row, 'Node') : null,
			suspended: cell(row, 'Suspend') === 'True' || cell(row, '.spec.suspend') === 'true',
			...(isNodes(resource) && cell(row, 'Status') !== null ? { cordoned: /SchedulingDisabled/.test(cell(row, 'Status') ?? '') } : {}),
			...(isServices(resource) && cell(row, 'Selector') !== null ? { selectorless: /^(|<none>)$/.test(cell(row, 'Selector') ?? '') } : {})
		};
	}

	/**
	 * Everything there is to do on a row, for the action bar, the row menu and the keys alike.
	 * `chosen` are the rows that copying and deleting work on: the marked ones, or the row itself.
	 */
	function actionsFor(row: Row | undefined, chosen: Row[]): MenuAction[] {
		// Rows from when the list was left may be gone; act only on the cluster's own list.
		if (!row || table.stale) return [];
		return objectActions(subject(row), actionOptions(row, chosen));
	}

	function actionOptions(row: Row, chosen: Row[]): ActionOptions {
		const cleanUp: MenuAction[] =
			isPods(resource) && canDelete(resource)
				? [{ group: 'Delete', label: t('Delete finished pods…'), keys: ['z'], danger: true, run: sanitize }]
				: [];
		return {
			show: (tab, extra) => open(row, tab, extra),
			enter: () => void enter(row),
			chosen: chosen.map(target),
			namespace,
			plugins: () => pick(row.key),
			extraDanger: cleanUp
		};
	}

	/** What there is to do on the marked rows together. */
	function markedActions(chosen: Row[]): MenuAction[] {
		if (table.stale) return [];
		return actionsForMarked({
			resource,
			chosen,
			marksProduction,
			writable,
			target,
			markAll: () => rows.forEach((r) => selection.add(r.key)),
			unmarkAll: () => selection.clear(),
			remove
		});
	}

	/** The row menu, from a right click or the row's ⋯ button. Marked rows go together. */
	function openMenu(row: Row, x: number, y: number, align: 'left' | 'right' = 'left', column?: DisplayColumn) {
		pick(row.key);
		const marked = chosenRows();
		const actions = selection.has(row.key) && marked.length > 1 ? markedActions(marked) : actionsFor(row, [row]);
		const withCell = column
			? withCellActions(actions, column.label, cellText(cells, row, column), () => rows.map((r) => cellText(cells, r, column)))
			: actions;
		app.menu = { actions: withCell, x, y, align };
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		const el = e.target as HTMLElement;
		if (forControl(e)) {
			if (el === filterInput && ['Escape', 'Enter', 'ArrowDown'].includes(e.key)) {
				if (e.key === 'Escape') filter = '';
				filterInput?.blur();
				e.preventDefault();
			}
			return;
		}
		const ctrl = e.ctrlKey || e.metaKey;
		// The row's menu, as a right click opens it.
		if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey && !ctrl)) {
			const at = list?.cursorMenuPoint();
			if (current && at) {
				e.preventDefault();
				openMenu(current, at.x, at.y, 'right');
			}
			return;
		}
		const handled = (() => {
			if (ctrl) {
				switch (e.key.toLowerCase()) {
					case 'a':
						rows.forEach((r) => selection.add(r.key));
						return true;
					case ' ':
						markRange();
						return true;
					case '\\':
						selection.clear();
						return true;
					case 's':
						exportCsv();
						return true;
					case 'z':
						if (!healthy && !faults) {
							app.toast(t('{kinds} have no state to tell which need attention', { kinds: place.label }));
							return true;
						}
						showTab(faults ? 'all' : 'attention');
						return true;
					// k9s pages with these; the browser's find would miss the rows not drawn anyway.
					case 'f':
					case 'b':
						if (app.details) return false;
						move(e.key.toLowerCase() === 'f' ? 20 : -20);
						return true;
				}
			} else if (e.altKey) {
				return false;
			} else {
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
						enter(current);
						return true;
					case 'f':
						if (!app.details) return false;
						app.layout.maximized = !app.layout.maximized;
						return true;
					case ' ':
						if (current) {
							toggleMark(current);
							move(1);
						}
						return true;
					case 'w':
						wide = !wide;
						return true;
					case '/':
						filterInput?.focus();
						return true;
					case 'N':
						return sortByColumn((_, format) => format === 'name');
					case 'A':
						return sortByColumn((name, format) => isAgeColumn({ name, format } as never));
					case 'S':
						return sortByColumn((name) => /^(status|phase|state)$/i.test(name));
					case 'P':
						if (!columns.some((c) => c.kind === 'namespace')) return false;
						sortBy(-1);
						return true;
					case 'R':
						return sortByColumn(named('ready'));
					case 'T':
						return sortByColumn(named('restarts'), true);
					case 'I':
						return sortByColumn(named('ip'));
					case 'O':
						return sortByColumn(named('node'));
					case 'C':
						return sortByMetric('CPU');
					case 'M':
						return sortByMetric('Mem');
					case 'X':
						return sortByMetric(isNodes(resource) ? '%CPU' : 'CPU / request');
					case 'Z':
						return sortByMetric(isNodes(resource) ? '%Mem' : 'Memory / request');
					case '0':
						app.namespace = '';
						return true;
					case 'Escape':
						if (app.details) {
							app.details = null;
							app.layout.maximized = false;
						} else if (selection.size) selection.clear();
						else if (filter) filter = '';
						else if (faults || runningOnly) showTab('all');
						// Back to the list the scope came from, as in k9s.
						else if (scope) {
							if (!app.go(-1)) app.scope = null;
						} else return false;
						return true;
				}
			}
			const chosen = chosenRows();
			// With several rows marked, s, r and c work on all of them.
			const actions = actionsFor(current, chosen);
			const key = keyOf(e);
			const action = actionFor(selection.size > 1 ? [...markedActions(chosen), ...actions] : actions, key);
			if (action) {
				action.run();
				return true;
			}
			// Read-only leaves the change keys out; say so instead of doing nothing.
			const refused = current && !table.stale ? refusedAction(subject(current), actionOptions(current, chosen), key) : undefined;
			if (refused) app.rails.explainReadOnly(refused.label);
			return !!refused;
		})();
		if (handled) e.preventDefault();
	}

	function remove(chosen: Row[]) {
		if (!chosen.length) return;
		// Without marks the dialog is for the row under the cursor: bring it into view first.
		if (!selection.size) void list?.revealCursor();
		app.dialog = { kind: 'delete', resource, targets: chosen.map(target) };
	}

	/**
	 * The width of the mark column. Before the list has columns it stretches over the whole table:
	 * its width then says nothing, and would hide columns and push the names aside.
	 */
	const markWidth = $derived(checkWidth && checkWidth < 80 ? checkWidth : 34);
	const hiddenColumns = $derived(table.columns.filter((c) => c.priority > 0).length);

	/**
	 * The columns drawn: those that fit the width, the least important left out first. All columns
	 * (w) draws every one and scrolls sideways, with the names kept in view. The mark and menu
	 * columns at both ends take 42 pixels besides the mark's width.
	 */
	const fitted = $derived(wide || !room ? { drawn: columns, left: [] as string[], nameMax: null } : fitColumns(cells, columns, rows, room - markWidth - 42));
	const drawn = $derived(fitted.drawn);
</script>

<svelte:window {onkeydown} />

<section>
	<ListHeader
		{resource}
		{scope}
		label={place.label}
		count={owned.length}
		bind:wide
		widens={!!(hiddenColumns || fitted.left.length || wide)}
		columns={allColumns}
		settings={columnSettings}
		settingsKey={columnKey}
		onexport={exportCsv}
	/>

	<ListNotices
		{problem}
		error={table.error}
		notice={table.notice}
		metrics={metricsKind ? metrics : null}
		{now}
		onretry={() => retry++}
	/>

	{#if productionHints.length && app.cluster}
		{@const context = app.cluster.context}
		<ProductionSuggestions
			{context}
			suggestions={productionHints}
			ondismiss={() => {
				hintDismissed = true;
				store(`kubri.productionHint.${context}`, 'off');
			}}
		/>
	{/if}

	<div class="card">
		<div class="toolbar">
			<label class="search">
				<Icon name="search" size={17} />
				<input
					bind:this={filterInput}
					bind:value={filter}
					aria-label={t('Filter {kinds}', { kinds: midSentence(place.label) })}
					placeholder={t('Filter by name, /regex/, -l app=web or -f text')}
				/>
				<kbd>/</kbd>
			</label>
		</div>
		<div class="tabs" role="tablist" aria-label={t('Show')} {@attach roving({ items: '[role="tab"]', orientation: 'horizontal', activate: true })}>
			<button role="tab" aria-selected={!faults && !runningOnly} onclick={() => showTab('all')}>
				{t('All')} <span class="n">{counts.all}</span>
			</button>
			{#if healthy || faults}
				<button role="tab" aria-selected={faults} onclick={() => showTab('attention')} title={t('Only rows that need a look (Ctrl+Z)')}>
					{t('Needs attention')} <span class="n" class:alert={counts.attention}>{counts.attention}</span>
				</button>
			{/if}
			{#if isPods(resource)}
				<button role="tab" aria-selected={runningOnly} onclick={() => showTab('running')}>
					{t('Running')} <span class="n">{counts.running}</span>
				</button>
			{/if}
		</div>

		<div class="actions">
			{#if selection.size}
				{@const marked = chosenRows()}
				<ActionBar actions={markedActions(marked)} marked>{t('{n} marked', { n: marked.length })}</ActionBar>
			{:else if current && app.details}
				<!-- The inspector holds the actions while it is open: one place, not two. -->
				<ActionBar actions={[]}>{current.namespace ? `${current.namespace}/` : ''}{current.name}</ActionBar>
			{:else if current && picked}
				<ActionBar actions={actionsFor(current, [current])}>{current.namespace ? `${current.namespace}/` : ''}{current.name}</ActionBar>
			{:else if current}
				{@const [before, after] = t('Click a row or press {key} to act on it').split('{key}')}
				<ActionBar actions={[]}><span class="choose">{before}<kbd>↓</kbd>{after}</span></ActionBar>
			{:else}
				<!-- An empty bar keeps its line, so the list does not move once rows come or go. -->
				<ActionBar actions={[]} />
			{/if}
		</div>

		<ResourceTable
			bind:this={list}
			bind:checkWidth
			bind:room
			{rows}
			columns={drawn}
			{cells}
			{cursor}
			{selection}
			{sort}
			nameMax={fitted.nameMax}
			{markWidth}
			stale={table.stale}
			{holding}
			onpick={(row) => pick(row.key)}
			onenter={(row) => void enter(row)}
			onopen={(row) => open(row)}
			onmark={toggleMark}
			onmenu={openMenu}
			onsort={(index) => sortBy(index)}
		>
			{#snippet after()}
				{#if table.loading || (holding && rows.length)}
					<p class="empty">{t('Loading…')}</p>
				{:else if rows.length === 0 && !problem}
					<p class="empty">
						{filter
							? t('Nothing matches the filter.')
							: faults
								? t('Nothing needs a look.')
								: runningOnly
									? t('No pods are running.')
									: scope
										? t('No {what}.', { what: scope.title })
										: ns
											? t('No {kind} objects in {namespace}.', { kind: resource.kind, namespace: ns })
											: t('No {kind} objects.', { kind: resource.kind })}
					</p>
					{#if !filter && !faults && !runningOnly && !scope && resource.verbs.includes('create') && !app.readOnly && !app.cluster?.snapshot}
						<p class="empty-action">
							<button onclick={() => app.showCreate(resource)}>{t('Create {kind}…', { kind: resource.kind })}</button>
						</p>
					{/if}
				{/if}
			{/snippet}
		</ResourceTable>
		<div class="foot">
			<span>{t('{n} of {total} {kinds}', { n: rows.length, total: owned.length, kinds: midSentence(place.label) })}</span>
			{#if table.stale}<span class="updating">{t('— updating…')}</span>{/if}
			<span class="end">
				{#if metricsKind}<span>{t('— not measured')}</span>{/if}
				{#if fitted.left.length}
					<span class="left-out" title={fitted.left.join(', ')}>
						{fitted.left.length === 1 ? t('{column} hidden to fit', { column: fitted.left[0] }) : t('{n} columns hidden to fit', { n: fitted.left.length })}
						<button onclick={() => (wide = true)}>{t('Show all')} <kbd>w</kbd></button>
					</span>
				{/if}
			</span>
		</div>
	</div>
</section>

<style>
	.choose {
		font-family: Inter, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
		font-size: 13px;
	}
	.choose kbd {
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
		font-size: 11px;
	}
	.updating {
		color: var(--text-muted);
	}
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
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
		padding: 14px 16px 6px;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 8px 0 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		color: var(--text-muted);
	}
	.search:focus-within {
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent);
	}
	.search input {
		flex: 1;
		min-width: 0;
		padding: 8px 0;
		border: none;
		background: none;
		color: var(--text);
		box-shadow: none;
	}
	kbd {
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
		font-size: 11px;
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
	.actions :global(.bar) {
		margin: 0;
		border: none;
		border-bottom: 1px solid var(--border-subtle);
		border-radius: 0;
		padding: 6px 12px;
	}
	.end {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 4px 16px;
		margin-left: auto;
	}
	.left-out {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.left-out button {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 11.5px;
		font-weight: 600;
	}
	.left-out kbd {
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 10.5px;
		font-weight: 400;
	}
	.empty-action {
		margin: -16px 0 0;
		text-align: center;
	}
	.empty-action button {
		padding: 7px 14px;
		border: 1px solid var(--accent);
		border-radius: 7px;
		background: var(--accent);
		color: var(--on-accent);
		font-size: 13px;
		font-weight: 600;
	}
	.empty {
		padding: 28px;
		text-align: center;
		color: var(--text-muted);
	}
	.foot {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		padding: 8px 16px;
		border-top: 1px solid var(--border-subtle);
		font-size: 11.5px;
		color: var(--text-muted);
	}

	@media (max-width: 720px) {
		section {
			padding: 0 12px 12px;
		}
		.tabs {
			gap: 14px;
			overflow-x: auto;
		}
	}
</style>

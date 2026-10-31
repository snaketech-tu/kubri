<script lang="ts">
	import { completedPods } from '../cleanup';
	import { forControl } from '../keys';
	import { tick, untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { canDelete, canRestart, canScale, copyLines, saveFile } from '../actions';
	import { type MetricsKind, type ObjectRef, type ResourceInfo, type Row, type Usage } from '../api';
	import { age, cpu, memory, relative } from '../format';
	import { actionFor, keyOf, type MenuAction } from '../menu';
	import { drill, objectActions, type Subject } from '../objects';
	import { isNodes, isPods, isServices } from '../pods';
	import { ALL_NAMESPACES, app, refOf, type DetailsExtra, type DetailsTab, type ListScope } from '../state.svelte';
	import { cellTone, type Tone } from '../status';
	import { filterRows, isAgeColumn, isFaulty, sortRows, toCsv, type SortState } from '../table';
	import { LiveMetrics, LiveTable } from '../watch.svelte';
	import { navPlace } from '../nav';
	import { columnsFor, defaultColumns, isShown, kindKey, ordered, pathsOf } from '../columns';
	import ActionBar from './ActionBar.svelte';
	import ColumnChooser from './ColumnChooser.svelte';
	import Icon, { type IconName } from './Icon.svelte';

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
	/** Only rows that need a look, like k9s Ctrl+Z. */
	let faults = $state(false);
	/** Only running pods. */
	let runningOnly = $state(false);
	let sort = $state<SortState>({ column: -1, descending: false });
	let sortedByUser = false;
	let cursorKey = $state<string | null>(null);
	/** Where Ctrl+Space marks a range from. */
	let anchorKey = $state<string | null>(null);
	let now = $state(Date.now());
	let filterInput = $state<HTMLInputElement>();
	let scroller = $state<HTMLDivElement>();

	const ns = $derived(resource.namespaced && namespace ? namespace : null);
	const metricsKind = $derived<MetricsKind | null>(isPods(resource) ? 'pods' : isNodes(resource) ? 'nodes' : null);
	const writable = $derived(!app.readOnly);

	/** How this kind's list shows its columns, from the settings. */
	const columnKey = $derived(kindKey(resource));
	const columnSettings = $derived(columnsFor(columnKey, app.settings.columns, app.viewOf(resource)));
	const paths = $derived(pathsOf(columnSettings));
	let chooserOpen = $state(false);

	$effect(() =>
		table.start(resource, ns, paths ? { ...(scope?.scope ?? {}), paths } : (scope?.scope ?? null), retry > 0)
	);
	$effect(() => {
		if (metricsKind) return metrics.start(metricsKind, ns);
	});
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	/** A column from metrics-server, with a negative index to sort by. */
	interface MetricColumn {
		label: string;
		index: number;
		wide: boolean;
		value: (u: Usage) => number | null | undefined;
		format: 'cpu' | 'memory' | 'percent';
		/** Which k9s threshold colors a percentage. */
		threshold?: 'cpu' | 'memory';
	}

	const percent = (used: number, of: number | null | undefined) => (of ? Math.round((used / of) * 100) : null);
	const CPU: MetricColumn = { label: 'CPU', index: -2, wide: false, value: (u) => u.cpu, format: 'cpu' };
	const MEM: MetricColumn = { label: 'Mem', index: -3, wide: false, value: (u) => u.memory, format: 'memory' };
	// As in k9s: usage against what the containers request and may use at most, and for nodes
	// against what they can give to pods.
	const POD_METRICS: MetricColumn[] = [
		CPU,
		MEM,
		{ label: 'CPU / request', index: -4, wide: false, value: (u) => percent(u.cpu, u.requests?.cpu), format: 'percent', threshold: 'cpu' },
		{ label: 'CPU / limit', index: -5, wide: true, value: (u) => percent(u.cpu, u.limits?.cpu), format: 'percent', threshold: 'cpu' },
		{ label: 'Memory / request', index: -6, wide: false, value: (u) => percent(u.memory, u.requests?.memory), format: 'percent', threshold: 'memory' },
		{ label: 'Memory / limit', index: -7, wide: true, value: (u) => percent(u.memory, u.limits?.memory), format: 'percent', threshold: 'memory' }
	];
	const NODE_METRICS: MetricColumn[] = [
		CPU,
		{ label: '%CPU', index: -8, wide: false, value: (u) => percent(u.cpu, u.allocatable?.cpu), format: 'percent', threshold: 'cpu' },
		MEM,
		{ label: '%Mem', index: -9, wide: false, value: (u) => percent(u.memory, u.allocatable?.memory), format: 'percent', threshold: 'memory' }
	];

	/** Empty cells, `<none>` and `None` all read as a dash. */
	const NOTHING = /^(|<none>|None)$/;

	const TIMESTAMP = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)$/;

	type DisplayColumn = (
		| { kind: 'namespace'; label: string; index: -1 }
		| { kind: 'cell'; label: string; index: number; age: boolean; name: boolean; custom: boolean }
		| ({ kind: 'metric' } & MetricColumn)
	) & { wideOnly: boolean };

	/** Every column the list can show, in their own order, before the settings and the wide view. */
	const allColumns = $derived.by(() => {
		const out: DisplayColumn[] = [];
		if (resource.namespaced && !ns) out.push({ kind: 'namespace', label: 'Namespace', index: -1, wideOnly: false });
		for (const [index, c] of table.columns.entries()) {
			const isAge = isAgeColumn(c);
			if (isAge && metricsKind) {
				const metricColumns = metricsKind === 'nodes' ? NODE_METRICS : POD_METRICS;
				out.push(...metricColumns.map((m) => ({ kind: 'metric' as const, ...m, wideOnly: m.wide })));
			}
			// A custom column comes back named by its path.
			const custom = c.format === 'jsonpath' ? columnSettings.custom.find((x) => x.path === c.name) : undefined;
			out.push({
				kind: 'cell',
				label: isAge ? 'Age' : (custom?.name ?? c.name),
				index,
				age: isAge,
				name: c.format === 'name',
				custom: c.format === 'jsonpath',
				wideOnly: c.priority > 0
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
	const faulty = $derived(owned.filter((r) => isFaulty(table.columns, r)));
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

	const place = $derived(navPlace(resource));
	/** Why the list may be wrong or empty: never shown as "no objects". */
	const problem = $derived(
		!table.error
			? null
			: table.hasLoaded
				? `Connection interrupted. Showing the rows from ${age(new Date(table.updated).toISOString(), now) || '0s'} ago.`
				: /forbidden|403|unauthorized|permission/i.test(table.error)
					? 'Access denied.'
					: 'Could not load the list.'
	);

	const rows = $derived.by(() => {
		const filtered = filterRows(scoped, filter);
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

	// Long lists render only the rows in view and a few around them, so 10,000 pods stay quick.
	const WINDOWED_FROM = 300;
	const OVERSCAN = 20;
	let scrollTop = $state(0);
	let viewHeight = $state(800);
	let rowHeight = $state(41);
	const windowed = $derived(rows.length > WINDOWED_FROM);
	const first = $derived(windowed ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN) : 0);
	const last = $derived(
		windowed ? Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / rowHeight) + OVERSCAN) : rows.length
	);
	const shown = $derived(windowed ? rows.slice(first, last) : rows);

	// Rows are as high as the zoom and font make them.
	$effect(() => {
		if (!shown.length) return;
		const height = scroller?.querySelector('tbody tr.row')?.getBoundingClientRect().height;
		if (height) untrack(() => (rowHeight = height));
	});

	/** Scrolls the row under the cursor into view, so a command never acts on a row out of sight. */
	function revealCursor() {
		if (windowed && scroller) {
			const head = scroller.querySelector('thead')?.getBoundingClientRect().height ?? 0;
			const top = cursor * rowHeight;
			if (top < scroller.scrollTop) scroller.scrollTop = top;
			else if (top + rowHeight > scroller.scrollTop + scroller.clientHeight - head)
				scroller.scrollTop = top + rowHeight - scroller.clientHeight + head;
		}
		return tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	}

	// The cursor stays in view when it moves, and when a filter or sort moves the rows around it.
	// Rows arriving from the cluster do not count: they must not pull the list away from where the user scrolled.
	$effect(() => {
		void cursorKey;
		void filter;
		void sort.column;
		void sort.descending;
		untrack(() => void revealCursor());
	});

	// Plugins and hotkeys run on the row under the cursor, with its cells as shown.
	$effect(() => {
		const focus = () => ({
			resource: refOf(resource),
			target: current ? target(current) : null,
			release: null,
			columns: current ? columns.map((c) => ({ name: c.label, value: cellText(current, c) })) : [],
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
		if (row) cursorKey = row.key;
		else app.toast(`${wanted.name} is not in the list`, 'bad');
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
		cursorKey = rows[next].key;
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
		const column = columns.find((c) => c.kind === 'metric' && c.label === label);
		if (!column) return false;
		sortBy(column.index, true);
		return true;
	}

	const named = (wanted: string) => (name: string) => name.toLowerCase() === wanted;

	function cell(row: Row, name: string): string | null {
		const index = table.columns.findIndex((c) => c.name === name);
		const value = index === -1 ? null : row.cells[index];
		return value === null || value === undefined ? null : String(value);
	}

	function metricValue(row: Row, column: MetricColumn) {
		const u = metrics.get(row.namespace, row.name);
		return u ? column.value(u) : null;
	}

	function cellText(row: Row, column: DisplayColumn): string {
		switch (column.kind) {
			case 'namespace':
				return row.namespace ?? '';
			case 'metric': {
				const value = metricValue(row, column);
				if (value === null || value === undefined) return '—';
				return column.format === 'cpu' ? cpu(value) : column.format === 'memory' ? memory(value) : `${value}%`;
			}
			case 'cell': {
				if (column.age) return age(row.created, app.snapshot?.taken ?? now);
				const value = row.cells[column.index];
				if (value === null || value === undefined) return '';
				if (column.label === 'Cluster-IP' && value === 'None') return 'Headless';
				// Times in custom columns, like a certificate's expiry, read as ages.
				if (column.custom && typeof value === 'string' && TIMESTAMP.test(value)) return relative(value, now);
				return String(value);
			}
		}
	}

	/** How close a pod is to its limit and request: red at 90 % of the limit, amber above the request. */
	function pressure(row: Row, what: 'cpu' | 'memory'): { tone: Tone; hint: string } | null {
		if (!isPods(resource)) return null;
		const u = metrics.get(row.namespace, row.name);
		if (!u) return null;
		const used = u[what];
		const limit = u.limits?.[what];
		const request = u.requests?.[what];
		const name = what === 'cpu' ? 'CPU' : 'memory';
		if (limit && used >= limit * 0.9) {
			return {
				tone: 'bad',
				hint:
					what === 'cpu'
						? `At ${Math.round((used / limit) * 100)} % of its CPU limit: it is being throttled`
						: `At ${Math.round((used / limit) * 100)} % of its memory limit: it may be killed`
			};
		}
		if (request && used > request) {
			return { tone: 'warn', hint: `Using more ${name} than it requests (${Math.round((used / request) * 100)} %)` };
		}
		return null;
	}

	/** Colors status cells, and percentages by the k9s thresholds. */
	function tone(row: Row, column: DisplayColumn, text: string): Tone {
		// A pod that finished is not "0 of 1 ready"; that is how it ends.
		if (column.kind === 'cell' && /^ready$/i.test(column.label) && /^(Completed|Succeeded)$/.test(cell(row, 'Status') ?? '')) {
			return '';
		}
		if (column.kind === 'cell') return cellTone(column.label, text);
		if (column.kind === 'metric' && (column.label === 'CPU' || column.label === 'Mem')) {
			return pressure(row, column.label === 'CPU' ? 'cpu' : 'memory')?.tone ?? '';
		}
		if (column.kind !== 'metric' || !column.threshold) return '';
		if (column.label.endsWith('/ request')) {
			const v = metricValue(row, column);
			return v !== null && v !== undefined && v > 100 ? 'warn' : '';
		}
		const value = metricValue(row, column);
		if (value === null || value === undefined) return '';
		const limits = app.k9s?.thresholds[column.threshold] ?? { warn: 70, critical: 90 };
		return value >= limits.critical ? 'bad' : value >= limits.warn ? 'warn' : '';
	}

	function open(row: Row | undefined, tab: DetailsTab = 'describe', extra: DetailsExtra = {}) {
		if (!row) return false;
		cursorKey = row.key;
		app.showDetails(resource, target(row), tab, extra);
		return true;
	}

	/** Enter: what belongs to the row, or else its description. */
	async function enter(row: Row | undefined) {
		if (!row) return;
		cursorKey = row.key;
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
			app.toast('No successfully completed pods in this view');
			return;
		}
		const statuses = Object.fromEntries(
			done.map((r) => [`${r.namespace ? `${r.namespace}/` : ''}${r.name}`, cell(r, 'Status') ?? ''])
		);
		app.dialog = { kind: 'delete', resource, targets: done.map(target), statuses };
	}

	/** An icon next to status words, so that the state does not hang on color alone. */
	function statusIcon(column: DisplayColumn, tone: Tone): IconName | null {
		if (column.kind !== 'cell' || !/^(status|phase|state)$/i.test(column.label)) return null;
		return tone === 'ok' ? 'ok' : tone === 'warn' ? 'pending' : tone === 'bad' ? 'critical' : null;
	}

	function exportCsv() {
		const header = columns.map((c) => c.label);
		const lines = rows.map((r) => columns.map((c) => cellText(r, c)));
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
		if (!row) return [];
		const cleanUp: MenuAction[] =
			isPods(resource) && canDelete(resource)
				? [{ group: 'Delete', label: 'Delete finished pods…', keys: ['z'], danger: true, run: sanitize }]
				: [];
		return objectActions(subject(row), {
			show: (tab, extra) => open(row, tab, extra),
			enter: () => void enter(row),
			chosen: chosen.map(target),
			namespace,
			plugins: () => (cursorKey = row.key),
			extraDanger: cleanUp
		});
	}

	/** What there is to do on the marked rows together. */
	function markedActions(chosen: Row[]): MenuAction[] {
		const out: MenuAction[] = [
			{ group: 'Marked', label: 'Copy names', keys: ['c'], primary: 0, run: () => copyLines(chosen.map((r) => r.name), ['name', 'names']) }
		];
		if (resource.namespaced) {
			out.push({
				group: 'Marked',
				label: 'Copy namespaces',
				keys: ['n'],
				primary: 0,
				run: () => copyLines([...new Set(chosen.map((r) => r.namespace ?? ''))], ['namespace', 'namespaces'])
			});
		}
		out.push(
			{ group: 'Marked', label: 'Mark all', keys: ['ctrl+a'], primary: 0, run: () => rows.forEach((r) => selection.add(r.key)) },
			{ group: 'Marked', label: 'Unmark all', keys: ['ctrl+\\'], primary: 0, run: () => selection.clear() }
		);
		if (writable && canScale(resource) && !isPods(resource)) {
			out.push({
				group: 'Change',
				label: `Scale ${chosen.length}…`,
				keys: ['s'],
				primary: 0,
				run: () => (app.dialog = { kind: 'scaleMany', resource, targets: chosen.map(target) })
			});
		}
		if (writable && canRestart(resource)) {
			out.push({
				group: 'Change',
				label: `Restart ${chosen.length}…`,
				keys: ['r'],
				primary: 0,
				run: () => (app.dialog = { kind: 'restartMany', resource, targets: chosen.map(target) })
			});
		}
		if (writable && canDelete(resource)) {
			out.push({
				group: 'Delete',
				label: `Delete ${chosen.length}…`,
				keys: ['ctrl+d', 'Delete'],
				danger: true,
				primary: 0,
				run: () => remove(chosen)
			});
		}
		return out;
	}

	/** The row menu, from a right click or the row's ⋯ button. Marked rows go together. */
	function openMenu(row: Row, x: number, y: number, align: 'left' | 'right' = 'left', column?: DisplayColumn) {
		cursorKey = row.key;
		const marked = chosenRows();
		const actions = selection.has(row.key) && marked.length > 1 ? markedActions(marked) : actionsFor(row, [row]);
		// The cell's copy actions join the Copy group, after what people do most: view, connect, change.
		const at = actions.findIndex((a) => a.group === 'Copy');
		const withCell = column
			? at === -1
				? [...actions, ...copyActions(row, column)]
				: [...actions.slice(0, at), ...copyActions(row, column), ...actions.slice(at)]
			: actions;
		app.menu = { actions: withCell, x, y, align };
	}

	/** Copying the cell right-clicked, or its whole column as the list shows it. */
	function copyActions(row: Row, column: DisplayColumn): MenuAction[] {
		const text = cellText(row, column);
		return [
			{
				group: 'Copy',
				label: text ? `Copy “${text.length > 32 ? `${text.slice(0, 31)}…` : text}”` : 'Copy cell',
				keys: [],
				run: () => copyLines([text], ['value', 'values'])
			},
			{
				group: 'Copy',
				label: `Copy the ${column.label} column`,
				keys: [],
				run: () => copyLines(rows.map((r) => cellText(r, column)), ['value', 'values'])
			}
		];
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
						app.maximized = !app.maximized;
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
							app.maximized = false;
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
			const action = actionFor(selection.size > 1 ? [...markedActions(chosen), ...actions] : actions, keyOf(e));
			action?.run();
			return !!action;
		})();
		if (handled) e.preventDefault();
	}

	function remove(chosen: Row[]) {
		if (!chosen.length) return;
		// Without marks the dialog is for the row under the cursor: bring it into view first.
		if (!selection.size) void revealCursor();
		app.dialog = { kind: 'delete', resource, targets: chosen.map(target) };
	}

	const hiddenColumns = $derived(table.columns.filter((c) => c.priority > 0).length);
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<nav class="crumbs" aria-label="Breadcrumb">
			<button onclick={() => app.showPulses()} title="Cluster overview">{app.contextName(app.cluster?.context ?? '')}</button>
			<Icon name="next" size={12} />
			<span>{!resource.namespaced ? 'cluster-wide' : app.namespace === ALL_NAMESPACES ? 'all namespaces' : app.namespace}</span>
			<Icon name="next" size={12} />
			{#if scope}
				<button onclick={() => (app.scope = null)} title="Show all {resource.kind} objects">{place.label}</button>
				<Icon name="next" size={12} />
				<span aria-current="page">{scope.title}</span>
			{:else}
				<span aria-current="page">{place.label}</span>
			{/if}
		</nav>
		<div class="heading">
			<div class="title">
				<h1 title="{resource.kind} · {resource.group ? `${resource.group}/` : ''}{resource.version}">{place.label}</h1>
				<span class="count">{owned.length}</span>
				{#if scope}
					<span class="chip">
						{scope.title}
						<button onclick={() => (app.scope = null)} title="Show all {resource.kind} objects" aria-label="Show all">×</button>
					</span>
				{/if}
			</div>
			<div class="tools">
				{#if hiddenColumns}
					<button class="toggle" class:on={wide} aria-pressed={wide} onclick={() => (wide = !wide)} title="Show all columns (w)">
						Wide
					</button>
				{/if}
				<div class="chooser">
					<button
						class="toggle"
						class:on={chooserOpen}
						aria-expanded={chooserOpen}
						onclick={() => (chooserOpen = !chooserOpen)}
						title="Choose, order and add columns for {resource.kind} lists"
					>
						Columns
					</button>
					{#if chooserOpen}
						<ColumnChooser
							kind={resource.kind}
							columns={allColumns}
							settings={columnSettings}
							defaults={defaultColumns(app.viewOf(resource))}
							{wide}
							onchange={(next) => app.setColumnSettings(columnKey, next)}
							onclose={() => (chooserOpen = false)}
						/>
					{/if}
				</div>
				<button class="toggle" onclick={exportCsv} title="Save the list as CSV (Ctrl+S)">
					<Icon name="download" size={15} /> CSV
				</button>
			</div>
		</div>
	</header>

	{#if problem}
		<div class="banner" role="alert">
			<Icon name="critical" />
			<span><strong>{problem}</strong> {table.error}</span>
			<button onclick={() => retry++}>Retry</button>
		</div>
	{/if}
	{#if table.notice}
		<div class="metrics-note" role="status">{table.notice}.</div>
	{/if}
	{#if metricsKind && metrics.error}
		<div class="metrics-note" role="status">
			Metrics {metrics.available ? 'are stale' : 'are unavailable'}{metrics.updated
				? `, last measured ${age(new Date(metrics.updated).toISOString(), now)} ago`
				: ''}: {metrics.error}
		</div>
	{/if}

	<div class="card">
		<div class="toolbar">
			<label class="search">
				<Icon name="search" size={17} />
				<input
					bind:this={filterInput}
					bind:value={filter}
					aria-label="Filter {place.label.toLowerCase()}"
					placeholder="Filter by name, /regex/, -l app=web or -f text"
				/>
				<kbd>/</kbd>
			</label>
		</div>
		<div class="tabs" role="tablist" aria-label="Show">
			<button role="tab" aria-selected={!faults && !runningOnly} onclick={() => showTab('all')}>
				All <span class="n">{counts.all}</span>
			</button>
			<button role="tab" aria-selected={faults} onclick={() => showTab('attention')} title="Only rows that need a look (Ctrl+Z)">
				Needs attention <span class="n" class:alert={counts.attention}>{counts.attention}</span>
			</button>
			{#if isPods(resource)}
				<button role="tab" aria-selected={runningOnly} onclick={() => showTab('running')}>
					Running <span class="n">{counts.running}</span>
				</button>
			{/if}
		</div>

		<div class="actions">
			{#if selection.size}
				{@const marked = chosenRows()}
				<ActionBar actions={markedActions(marked)} marked>{marked.length} marked</ActionBar>
			{:else if current}
				<ActionBar actions={actionsFor(current, [current])}>{current.namespace ? `${current.namespace}/` : ''}{current.name}</ActionBar>
			{/if}
		</div>

		<div
			class="table-wrap"
			bind:this={scroller}
			bind:clientHeight={viewHeight}
			onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
		>
			<table aria-rowcount={windowed ? rows.length + 1 : undefined}>
				<thead>
					<tr>
						<th class="check"></th>
						{#each columns as column (column.label + column.index)}
							<th
								class:numeric={column.kind === 'metric'}
								class:sorted={sort.column === column.index}
								aria-sort={sort.column === column.index ? (sort.descending ? 'descending' : 'ascending') : 'none'}
							>
								<button class="sort" onclick={() => sortBy(column.index)}>
									{column.label.replace(/^Memory/, 'Mem')}
									<span class="arrow" aria-hidden="true">
										{sort.column === column.index ? (sort.descending ? '↓' : '↑') : '↕'}
									</span>
								</button>
							</th>
						{/each}
						<th class="more" aria-label="Actions"></th>
					</tr>
				</thead>
				<tbody>
					{#if first > 0}
						<tr class="spacer" aria-hidden="true" style:height="{first * rowHeight}px"><td colspan={columns.length + 2}></td></tr>
					{/if}
					{#each shown as row, j (row.key)}
						{@const i = first + j}
						<tr
							class="row"
							aria-rowindex={windowed ? i + 2 : undefined}
							class:cursor={i === cursor}
							class:checked={selection.has(row.key)}
							onclick={() => (cursorKey = row.key)}
							ondblclick={() => enter(row)}
							oncontextmenu={(e) => {
								e.preventDefault();
								openMenu(row, e.clientX, e.clientY);
							}}
						>
							<td class="check">
								<input
									type="checkbox"
									aria-label="Mark {row.name}"
									checked={selection.has(row.key)}
									onclick={(e) => {
										e.stopPropagation();
										toggleMark(row);
									}}
								/>
							</td>
							{#each columns as column (column.label + column.index)}
								{@const raw = cellText(row, column)}
								{@const text = column.kind === 'cell' && !column.name && NOTHING.test(raw) ? '—' : raw}
								{@const color = tone(row, column, raw)}
								{@const hint = column.kind === 'metric' && (column.label === 'CPU' || column.label === 'Mem') ? pressure(row, column.label === 'CPU' ? 'cpu' : 'memory')?.hint : undefined}
								{@const icon = statusIcon(column, color)}
								<td
									oncontextmenu={(e) => {
										e.preventDefault();
										e.stopPropagation();
										openMenu(row, e.clientX, e.clientY, 'left', column);
									}}
									class={color}
									class:wrap={column.kind === 'cell' && column.label === 'Message'}
									title={hint}
									class:name={column.kind === 'cell' && column.name}
									class:muted={column.kind === 'namespace' || (column.kind === 'cell' && column.age)}
									class:numeric={column.kind === 'metric'}
								>
									{#if column.kind === 'cell' && column.name}
										<button
											class="open"
											title="Describe {row.name}"
											onclick={(e) => {
												e.stopPropagation();
												open(row);
											}}>{text}</button
										>
									{:else if icon}
										<span class="status"><Icon name={icon} size={15} />{text}</span>
									{:else if column.kind === 'cell' && column.label === 'Message'}
										<span class="clamp" title={text}>{text}</span>
									{:else}
										{text}
									{/if}
								</td>
							{/each}
							<td class="more">
								<button
									aria-label="Actions for {row.name}"
									title="Actions (right click)"
									onclick={(e) => {
										e.stopPropagation();
										const rect = e.currentTarget.getBoundingClientRect();
										openMenu(row, rect.right, rect.bottom + 4, 'right');
									}}
									ondblclick={(e) => e.stopPropagation()}>⋯</button
								>
							</td>
						</tr>
					{/each}
					{#if last < rows.length}
						<tr class="spacer" aria-hidden="true" style:height="{(rows.length - last) * rowHeight}px"
							><td colspan={columns.length + 2}></td></tr
						>
					{/if}
				</tbody>
			</table>
			{#if table.loading}
				<p class="empty">Loading…</p>
			{:else if rows.length === 0 && !problem}
				<p class="empty">
					{filter
						? 'Nothing matches the filter.'
						: faults
							? 'Nothing needs a look.'
							: runningOnly
								? 'No pods are running.'
								: scope
									? `No ${scope.title}.`
									: `No ${resource.kind} objects${ns ? ` in ${ns}` : ''}.`}
				</p>
			{/if}
		</div>
		<div class="foot">
			<span>{rows.length} of {owned.length} {place.label.toLowerCase()}</span>
			{#if metricsKind}<span>— not measured</span>{/if}
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
	header {
		padding: 20px 0 16px;
	}
	.crumbs {
		display: flex;
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
	.banner {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		margin-bottom: 12px;
		padding: 10px 14px;
		border: 1px solid color-mix(in srgb, var(--bad) 30%, var(--border));
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, var(--surface-1));
		color: var(--bad);
		font-size: 13px;
	}
	.banner span {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.banner button {
		padding: 5px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-weight: 600;
	}
	.metrics-note {
		margin-bottom: 12px;
		font-size: 12px;
		color: var(--text-muted);
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
	.chooser {
		position: relative;
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
	.table-wrap {
		flex: 1;
		min-height: 0;
		overflow: auto;
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
		padding: 0 8px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.sort {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 9px 0;
		border: none;
		background: none;
		color: inherit;
		font: inherit;
	}
	th:hover,
	th.sorted {
		color: var(--text);
	}
	.arrow {
		font-size: 10px;
		opacity: 0.45;
	}
	th.sorted .arrow {
		opacity: 1;
	}
	th.numeric .sort {
		flex-direction: row-reverse;
	}
	.spacer td {
		height: auto;
		padding: 0;
		border: none;
	}
	td {
		height: 40px;
		padding: 4px 8px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		max-width: 420px;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.check {
		width: 34px;
		padding: 0 0 0 14px;
	}
	.check input {
		opacity: 0;
		margin: 0;
	}
	.check input:focus-visible {
		opacity: 1;
	}
	.more {
		width: 40px;
		padding: 2px 10px 2px 6px;
		text-align: right;
	}
	.more button {
		width: 26px;
		height: 24px;
		padding: 0;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 15px;
		line-height: 1;
		opacity: 0;
	}
	tr:hover .more button,
	tr.cursor .more button,
	.more button:focus-visible {
		opacity: 1;
	}
	.more button:hover {
		border-color: var(--border);
		background: var(--surface-2);
		color: var(--text);
	}
	tr:hover .check input,
	tr.cursor .check input,
	tr.checked .check input {
		opacity: 1;
	}
	tbody tr {
		cursor: default;
	}
	tbody tr.row:hover {
		background: color-mix(in srgb, var(--surface-2) 55%, transparent);
	}
	tbody tr.checked {
		background: color-mix(in srgb, var(--accent) 7%, transparent);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.open {
		max-width: 100%;
		padding: 0;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-weight: 600;
		text-align: left;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.open:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	/* Event messages take two lines instead of one long line; the title has all of it. */
	td.wrap {
		min-width: 260px;
		max-width: 520px;
	}
	td.wrap .clamp {
		white-space: normal;
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		line-height: 1.3;
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
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
		padding: 28px;
		text-align: center;
		color: var(--text-muted);
	}
	.foot {
		display: flex;
		justify-content: space-between;
		gap: 12px;
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

<script lang="ts">
	import { forControl } from '../keys';
	import { tick } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import {
		canDelete,
		canEdit,
		canRestart,
		canScale,
		copyLines,
		cordon,
		portForward,
		saveFile,
		startPlugin
	} from '../actions';
	import { backend, type MetricsKind, type ObjectRef, type ResourceInfo, type Row, type Usage } from '../api';
	import { age, cpu, errorMessage, memory } from '../format';
	import { parseShortcut, pluginApplies, shortcutLabel } from '../k9s';
	import { actionFor, keyOf, type MenuAction } from '../menu';
	import {
		canPortForward,
		hasImages,
		hasLogs,
		hasUsers,
		isCRDs,
		isCronJobs,
		isNamespaces,
		isNodes,
		isPods,
		isReplicaSets,
		isSecrets
	} from '../pods';
	import {
		app,
		findResource,
		refOf,
		type DetailsExtra,
		type Dialog,
		type DetailsTab,
		type ListScope
	} from '../state.svelte';
	import { cellTone, type Tone } from '../status';
	import { filterRows, isAgeColumn, isFaulty, sortRows, toCsv, type SortState } from '../table';
	import { LiveMetrics, LiveTable } from '../watch.svelte';
	import ActionBar from './ActionBar.svelte';

	let {
		resource,
		namespace,
		scope = null
	}: { resource: ResourceInfo; namespace: string; scope?: ListScope | null } = $props();

	const table = new LiveTable();
	const metrics = new LiveMetrics();
	const selection = new SvelteSet<string>();
	let filter = $state('');
	let wide = $state(false);
	/** Only rows that need a look, like k9s Ctrl+Z. */
	let faults = $state(false);
	let sort = $state<SortState>({ column: -1, descending: false });
	let cursorKey = $state<string | null>(null);
	/** Where Ctrl+Space marks a range from. */
	let anchorKey = $state<string | null>(null);
	let now = $state(Date.now());
	let filterInput = $state<HTMLInputElement>();
	let scroller = $state<HTMLDivElement>();

	const ns = $derived(resource.namespaced && namespace ? namespace : null);
	const metricsKind = $derived<MetricsKind | null>(isPods(resource) ? 'pods' : isNodes(resource) ? 'nodes' : null);
	const writable = $derived(!app.readOnly);

	$effect(() => table.start(resource, ns, scope?.scope ?? null));
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
		{ label: '%CPU/R', index: -4, wide: false, value: (u) => percent(u.cpu, u.requests?.cpu), format: 'percent', threshold: 'cpu' },
		{ label: '%CPU/L', index: -5, wide: true, value: (u) => percent(u.cpu, u.limits?.cpu), format: 'percent', threshold: 'cpu' },
		{ label: '%Mem/R', index: -6, wide: false, value: (u) => percent(u.memory, u.requests?.memory), format: 'percent', threshold: 'memory' },
		{ label: '%Mem/L', index: -7, wide: true, value: (u) => percent(u.memory, u.limits?.memory), format: 'percent', threshold: 'memory' }
	];
	const NODE_METRICS: MetricColumn[] = [
		CPU,
		{ label: '%CPU', index: -8, wide: false, value: (u) => percent(u.cpu, u.allocatable?.cpu), format: 'percent', threshold: 'cpu' },
		MEM,
		{ label: '%Mem', index: -9, wide: false, value: (u) => percent(u.memory, u.allocatable?.memory), format: 'percent', threshold: 'memory' }
	];

	type DisplayColumn =
		| { kind: 'namespace'; label: string; index: -1 }
		| { kind: 'cell'; label: string; index: number; age: boolean; name: boolean }
		| ({ kind: 'metric' } & MetricColumn);

	const columns = $derived.by(() => {
		const out: DisplayColumn[] = [];
		if (resource.namespaced && !ns) out.push({ kind: 'namespace', label: 'Namespace', index: -1 });
		const cells = table.columns
			.map((c, index) => ({ c, index }))
			.filter(({ c }) => wide || c.priority === 0);
		for (const { c, index } of cells) {
			const isAge = isAgeColumn(c);
			if (isAge && metrics.available) {
				const metricColumns = metricsKind === 'nodes' ? NODE_METRICS : POD_METRICS;
				out.push(...metricColumns.filter((m) => wide || !m.wide).map((m) => ({ kind: 'metric' as const, ...m })));
			}
			out.push({ kind: 'cell', label: c.name, index, age: isAge, name: c.format === 'name' });
		}
		return out;
	});

	/** The rows of this view: what the scope's owner owns, if it names one. */
	const owned = $derived.by(() => {
		const owner = scope?.ownerUid;
		return owner ? table.rows.filter((r) => r.owners.some((o) => o.uid === owner)) : table.rows;
	});
	/** With Ctrl+Z, only the rows that need a look. */
	const scoped = $derived(faults ? owned.filter((r) => isFaulty(table.columns, r)) : owned);

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

	$effect(() => {
		void cursorKey;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
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
		return { resource: refOf(resource), namespace: row.namespace, name: row.name };
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
				if (value === null || value === undefined) return '';
				return column.format === 'cpu' ? cpu(value) : column.format === 'memory' ? memory(value) : `${value}%`;
			}
			case 'cell': {
				if (column.age) return age(row.created, now);
				const value = row.cells[column.index];
				return value === null || value === undefined ? '' : String(value);
			}
		}
	}

	/** Colors status cells, and percentages by the k9s thresholds. */
	function tone(row: Row, column: DisplayColumn, text: string): Tone {
		if (column.kind === 'cell') return cellTone(column.label, text);
		if (column.kind !== 'metric' || !column.threshold) return '';
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

	/**
	 * Enter, as in k9s: from a namespace to its pods, from a CRD to its objects, from a workload,
	 * CronJob, Service or node to what belongs to it (or where `jumps.yaml` says); else describe.
	 */
	async function enter(row: Row | undefined) {
		if (!row) return;
		cursorKey = row.key;
		if (isNamespaces(resource)) {
			const pods = findResource(app.resources, { group: '', version: 'v1', plural: 'pods' });
			if (pods) return app.open(pods, row.name, '');
		}
		if (isCRDs(resource)) {
			const [plural, ...group] = row.name.split('.');
			const defined = app.resources.find((r) => r.plural === plural && r.group === group.join('.'));
			if (defined) return app.open(defined, undefined, '');
		}
		if (!isPods(resource) && canDrill()) {
			try {
				const drill = await backend.drill(target(row));
				if (drill) return app.drill(drill);
			} catch (err) {
				app.toast(errorMessage(err), 'bad');
				return;
			}
		}
		open(row);
	}

	/** Kinds Kubri drills into, and those `jumps.yaml` names. */
	function canDrill() {
		const builtin: Record<string, string[]> = {
			'': ['services', 'replicationcontrollers', 'nodes'],
			apps: ['deployments', 'replicasets', 'statefulsets', 'daemonsets'],
			batch: ['jobs', 'cronjobs']
		};
		if (builtin[resource.group]?.includes(resource.plural)) return true;
		return (app.k9s?.jumps ?? []).some((j) => {
			const parts = j.source.split('/');
			const [group, plural] = parts.length === 3 ? [parts[0], parts[2]] : ['', parts[1]];
			return group === resource.group && plural === resource.plural;
		});
	}

	/** Shows the controller of the row, like k9s Shift+J. */
	function jumpToOwner(row: Row) {
		const owner = row.owners.find((o) => o.controller) ?? row.owners[0];
		if (!owner) {
			app.toast(`${row.name} has no owner`, 'bad');
			return;
		}
		const group = owner.apiVersion.includes('/') ? owner.apiVersion.split('/')[0] : '';
		const kinds = app.resources.filter((r) => r.group === group && r.kind === owner.kind);
		const version = owner.apiVersion.split('/').pop();
		const kind = kinds.find((r) => r.version === version) ?? kinds[0];
		if (!kind) {
			app.toast(`The cluster does not serve ${owner.kind}`, 'bad');
			return;
		}
		app.openAt(kind, row.namespace, owner.name);
	}

	/** The ReplicaSets of a Deployment, like k9s `z`. */
	async function showReplicaSets(row: Row) {
		const replicasets = findResource(app.resources, { group: 'apps', version: 'v1', plural: 'replicasets' });
		if (!replicasets) return;
		try {
			const uid = (await backend.getObject(target(row))).metadata.uid;
			if (!uid) return;
			app.open(replicasets, row.namespace ?? undefined, '', {
				title: `ReplicaSets of Deployment ${row.name}`,
				scope: {},
				ownerUid: uid
			});
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
	}

	/** Pods in a state k9s sanitizes away with `z`: done, failed or stuck. */
	const SANITIZED = /^(Completed|Pending|CrashLoopBackOff|Error|ImagePullBackOff|ContainerStatusUnknown|Evicted|OOMKilled)$/;

	function sanitize() {
		const done = rows.filter((r) => SANITIZED.test(cell(r, 'Status') ?? ''));
		if (!done.length) {
			app.toast('No pods are completed, failed or stuck', 'bad');
			return;
		}
		app.dialog = { kind: 'delete', resource, targets: done.map(target) };
	}

	function exportCsv() {
		const header = columns.map((c) => c.label);
		const lines = rows.map((r) => columns.map((c) => cellText(r, c)));
		const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15);
		saveFile(`${resource.plural}${ns ? `-${ns}` : ''}-${stamp}.csv`, toCsv(header, lines));
	}

	/** What the drill-down of Enter shows for this kind, or null where Enter only describes. */
	function drillLabel(): string | null {
		if (isNamespaces(resource)) return 'Show pods';
		if (isCRDs(resource)) return 'Show objects';
		if (isPods(resource) || !canDrill()) return null;
		if (isCronJobs(resource)) return 'Show jobs';
		const builtin = ['services', 'replicationcontrollers', 'nodes', 'deployments', 'replicasets', 'statefulsets', 'daemonsets', 'jobs'];
		return builtin.includes(resource.plural) ? 'Show pods' : 'Open';
	}

	/**
	 * Everything there is to do on a row, for the action bar, the row menu and the keys alike.
	 * `chosen` are the rows that copying and deleting work on: the marked ones, or the row itself.
	 */
	function actionsFor(row: Row | undefined, chosen: Row[]): MenuAction[] {
		if (!row) return [];
		const out: MenuAction[] = [];
		const add = (group: string, label: string, keys: string[], run: () => void, more: Partial<MenuAction> = {}) =>
			out.push({ group, label, keys, run, ...more });
		const show = (tab: DetailsTab, extra: DetailsExtra = {}) => () => void open(row, tab, extra);
		const dialog = (make: () => Dialog) => () => {
			cursorKey = row.key;
			app.dialog = make();
		};
		const ref = target(row);

		const drill = drillLabel();
		if (drill) add('View', drill, [], () => enter(row), { hint: 'enter', primary: 1 });
		add('View', 'Describe', ['d'], show('describe'), { primary: 5 });
		add('View', 'YAML', ['y'], show('yaml'), { primary: 6 });
		if (hasLogs(resource)) add('View', 'Logs', ['l'], show('logs'), { primary: 2 });
		if (isPods(resource)) add('View', 'Previous logs', ['p'], show('logs', { previous: true }));
		if (isSecrets(resource)) add('View', 'Decode', ['x'], show('describe', { decoded: true }));
		if (hasUsers(resource)) add('View', 'Used by', ['u'], show('describe'));

		if (isPods(resource) && writable) {
			add('Connect', 'Shell', ['s'], show('shell', { mode: 'shell' }), { primary: 3 });
			add('Connect', 'Attach', ['a'], show('shell', { mode: 'attach' }));
		}
		if (canPortForward(resource)) {
			add('Connect', 'Port-forward…', ['F'], () => void portForward(resource, ref), { primary: 8 });
		}

		if (writable) {
			if (isNodes(resource)) {
				add('Change', 'Cordon', ['c'], () => void cordon(row.name, true), { primary: 4 });
				add('Change', 'Uncordon', ['u'], () => void cordon(row.name, false), { primary: 4 });
				add('Change', 'Drain…', ['r'], dialog(() => ({ kind: 'drain', resource, target: ref })), { primary: 4 });
			}
			if (isCronJobs(resource)) {
				add('Change', 'Trigger now…', ['t'], dialog(() => ({ kind: 'trigger', resource, target: ref })), { primary: 4 });
				const suspend = cell(row, 'Suspend') !== 'True';
				add('Change', suspend ? 'Suspend…' : 'Resume…', ['s'], dialog(() => ({ kind: 'suspend', resource, target: ref, suspend })));
			}
			if (canEdit(resource)) add('Change', 'Edit', ['e'], show('yaml', { editing: true }), { primary: 7 });
			if (canScale(resource) && !isPods(resource)) {
				add('Change', 'Scale…', ['s'], dialog(() => ({ kind: 'scale', resource, target: ref })), { primary: 4 });
			}
			if (canRestart(resource)) {
				add('Change', 'Restart…', ['r'], dialog(() => ({ kind: 'restart', resource, target: ref })), { primary: 4 });
			}
			if (hasImages(resource)) add('Change', 'Set image…', ['i'], dialog(() => ({ kind: 'image', resource, target: ref })));
			if (isReplicaSets(resource)) {
				add('Change', 'Roll back to this…', ['ctrl+l'], dialog(() => ({ kind: 'rollback', resource, target: ref })));
			}
			if (resource.verbs.includes('patch')) {
				add('Change', 'Labels & annotations…', ['L'], dialog(() => ({ kind: 'labels', resource, target: ref })));
			}
		}

		const node = isPods(resource) ? cell(row, 'Node') : null;
		const nodes = findResource(app.resources, { group: '', version: 'v1', plural: 'nodes' });
		if (node && node !== '<none>' && nodes) add('Go to', `Node ${node}`, ['o'], () => app.openAt(nodes, null, node));
		if (row.owners.length) {
			const owner = row.owners.find((o) => o.controller) ?? row.owners[0];
			add('Go to', `Owner ${owner.kind} ${owner.name}`, ['J'], () => jumpToOwner(row));
		}
		if (resource.group === 'apps' && resource.plural === 'deployments') {
			add('Go to', 'ReplicaSets', ['z'], () => void showReplicaSets(row));
		}
		if (isNamespaces(resource)) {
			add('Go to', `Switch to ${row.name}`, ['u'], () => {
				app.namespace = row.name;
				app.toast(`Namespace ${row.name}`);
			});
		} else if (resource.namespaced && row.namespace && row.namespace !== namespace) {
			const ns = row.namespace;
			add('Go to', `Switch to namespace ${ns}`, ['W'], () => (app.namespace = ns));
		}

		const several = chosen.length > 1;
		add('Copy', several ? `Copy ${chosen.length} names` : 'Copy name', ['c'], () =>
			copyLines(
				chosen.map((r) => r.name),
				['name', 'names']
			)
		);
		if (resource.namespaced) {
			add('Copy', several ? 'Copy namespaces' : 'Copy namespace', ['n'], () =>
				copyLines(
					[...new Set(chosen.map((r) => r.namespace ?? ''))],
					['namespace', 'namespaces']
				)
			);
		}

		for (const plugin of app.k9s?.plugins ?? []) {
			const shortcut = parseShortcut(plugin.shortCut);
			if (!pluginApplies(plugin, app.pluginView) || (plugin.dangerous && !writable)) continue;
			add('Plugins', plugin.description || plugin.name, [], () => {
				cursorKey = row.key;
				startPlugin(plugin);
			}, { hint: shortcut ? shortcutLabel(shortcut) : undefined });
		}

		if (writable && isPods(resource) && canDelete(resource)) {
			add('Delete', 'Delete finished pods…', ['z'], sanitize, { danger: true });
		}
		if (writable && canDelete(resource)) {
			const label = several ? `Delete ${chosen.length}…` : 'Delete…';
			add('Delete', label, ['ctrl+d', 'Delete'], () => remove(chosen), { danger: true, primary: 0 });
		}
		return out;
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
	function openMenu(row: Row, x: number, y: number, align: 'left' | 'right' = 'left') {
		cursorKey = row.key;
		const marked = chosenRows();
		const actions = selection.has(row.key) && marked.length > 1 ? markedActions(marked) : actionsFor(row, [row]);
		app.menu = { actions, x, y, align };
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
						faults = !faults;
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
						return sortByMetric(isNodes(resource) ? '%CPU' : '%CPU/R');
					case 'Z':
						return sortByMetric(isNodes(resource) ? '%Mem' : '%Mem/R');
					case '0':
						app.namespace = '';
						return true;
					case 'Escape':
						if (app.details) {
							app.details = null;
							app.maximized = false;
						} else if (selection.size) selection.clear();
						else if (filter) filter = '';
						else if (faults) faults = false;
						// Back to the list the scope came from, as in k9s.
						else if (scope) {
							if (!app.go(-1)) app.scope = null;
						} else return false;
						return true;
				}
			}
			const action = actionFor(actionsFor(current, chosenRows()), keyOf(e));
			action?.run();
			return !!action;
		})();
		if (handled) e.preventDefault();
	}

	function remove(chosen: Row[]) {
		if (!chosen.length) return;
		app.dialog = { kind: 'delete', resource, targets: chosen.map(target) };
	}

	const hiddenColumns = $derived(table.columns.filter((c) => c.priority > 0).length);
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<div class="title">
			<h1>{resource.kind}</h1>
			<span class="count">{rows.length}{rows.length !== owned.length ? ` / ${owned.length}` : ''}</span>
			<code>{resource.group ? `${resource.group}/` : ''}{resource.version}</code>
			{#if scope}
				<span class="chip">
					{scope.title}
					<button onclick={() => (app.scope = null)} title="Show all {resource.kind} objects" aria-label="Show all">×</button>
				</span>
			{/if}
			{#if faults}
				<span class="chip bad">
					Faults
					<button onclick={() => (faults = false)} title="Show all rows (Ctrl+Z)" aria-label="Show all rows">×</button>
				</span>
			{/if}
		</div>
		<div class="tools">
			{#if hiddenColumns}
				<button class="toggle" class:on={wide} aria-pressed={wide} onclick={() => (wide = !wide)} title="Show all columns (w)">Wide</button>
			{/if}
			<button class="toggle" class:on={faults} aria-pressed={faults} onclick={() => (faults = !faults)} title="Only rows that need a look (Ctrl+Z)">
				Faults
			</button>
			<button class="toggle" onclick={exportCsv} title="Save the list as CSV (Ctrl+S)">CSV</button>
			<input class="filter" bind:this={filterInput} bind:value={filter} placeholder="Filter…  (/)   regex, !, -f, -l app=web" />
		</div>
	</header>

	{#if table.error}
		<div class="banner">{table.error}</div>
	{/if}

	{#if selection.size}
		{@const marked = chosenRows()}
		<ActionBar actions={markedActions(marked)} marked>{marked.length} marked</ActionBar>
	{:else if current}
		<ActionBar actions={actionsFor(current, [current])}>{current.name}</ActionBar>
	{/if}

	<div class="table-wrap" bind:this={scroller}>
		<table>
			<thead>
				<tr>
					<th class="check"></th>
					{#each columns as column (column.label + column.index)}
						<th
							class:numeric={column.kind === 'metric'}
							class:sorted={sort.column === column.index}
							onclick={() => sortBy(column.index)}
						>
							{column.label}
							{#if sort.column === column.index}<span class="arrow">{sort.descending ? '↓' : '↑'}</span>{/if}
						</th>
					{/each}
					<th class="more" aria-label="Actions"></th>
				</tr>
			</thead>
			<tbody>
				{#each rows as row, i (row.key)}
					<tr
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
								tabindex="-1"
								checked={selection.has(row.key)}
								onclick={(e) => {
									e.stopPropagation();
									toggleMark(row);
								}}
							/>
						</td>
						{#each columns as column (column.label + column.index)}
							{@const text = cellText(row, column)}
							<td
								class={tone(row, column, text)}
								class:name={column.kind === 'cell' && column.name}
								class:muted={column.kind === 'namespace' || (column.kind === 'cell' && column.age)}
								class:numeric={column.kind === 'metric'}
							>
								{text}
							</td>
						{/each}
						<td class="more">
							<button
								tabindex="-1"
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
			</tbody>
		</table>
		{#if table.loading}
			<p class="empty">Loading…</p>
		{:else if rows.length === 0}
			<p class="empty">
				{filter
					? 'Nothing matches the filter.'
					: faults
						? 'Nothing needs a look.'
						: scope
							? `No ${scope.title}.`
							: `No ${resource.kind} objects${ns ? ` in ${ns}` : ''}.`}
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
	.chip.bad {
		border-color: color-mix(in srgb, var(--bad) 45%, transparent);
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
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
	.more {
		width: 34px;
		padding: 2px 6px;
		text-align: right;
	}
	.more button {
		width: 26px;
		height: 22px;
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
	tr.cursor .more button {
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

// What the cells of a resource list say, and how they look: the columns from metrics-server, the
// text of each cell, its color and its hints. Kept out of ResourceView so it can be tested.
import type { Column, K9sConfig, MetricsKind, ResourceInfo, Row, Usage } from './api';
import { cpu, cpuText, memory, memoryText, age, relative, restarts } from './format';
import { describeCron } from './cron';
import { t } from './i18n';
import type { IconName } from './components/Icon.svelte';
import { columnRank, columnWidth, overflowing } from './columns';
import { limitSuffix, type Measure } from './limits';
import { isPods } from './pods';
import type { ProductionRule } from './production';
import { cellTone, type Tone } from './status';
import { units } from './units.svelte';

/** A column from metrics-server, with a negative index to sort by. */
export interface MetricColumn {
	label: string;
	index: number;
	wide: boolean;
	value: (u: Usage) => number | null | undefined;
	format: 'cpu' | 'memory' | 'percent' | 'pods';
	/** Which k9s threshold colors a percentage. */
	threshold?: 'cpu' | 'memory';
}

const percent = (used: number, of: number | null | undefined) => (of ? Math.round((used / of) * 100) : null);
const free = (of: number | undefined, requested: number | undefined) =>
	of === undefined || requested === undefined ? null : Math.max(0, of - requested);
const CPU: MetricColumn = { label: 'CPU', index: -2, wide: false, value: (u) => u.cpu, format: 'cpu' };
const MEM: MetricColumn = { label: 'Mem', index: -3, wide: false, value: (u) => u.memory, format: 'memory' };
// As in k9s: usage against what the containers request and may use at most, and for nodes
// against what they can give to pods.
const POD_METRICS: MetricColumn[] = [
	CPU,
	MEM,
	{ label: 'CPU / request', index: -4, wide: true, value: (u) => percent(u.cpu, u.requests?.cpu), format: 'percent', threshold: 'cpu' },
	{ label: 'CPU / limit', index: -5, wide: true, value: (u) => percent(u.cpu, u.limits?.cpu), format: 'percent', threshold: 'cpu' },
	{ label: 'Memory / request', index: -6, wide: true, value: (u) => percent(u.memory, u.requests?.memory), format: 'percent', threshold: 'memory' },
	{ label: 'Memory / limit', index: -7, wide: true, value: (u) => percent(u.memory, u.limits?.memory), format: 'percent', threshold: 'memory' },
	// What the pod spec sets, summed over the containers; a limit only when every container has one.
	{ label: 'CPU request', index: -10, wide: true, value: (u) => u.requests?.cpu, format: 'cpu' },
	{ label: 'CPU limit', index: -11, wide: true, value: (u) => u.limits?.cpu, format: 'cpu' },
	{ label: 'Memory request', index: -12, wide: true, value: (u) => u.requests?.memory, format: 'memory' },
	{ label: 'Memory limit', index: -13, wide: true, value: (u) => u.limits?.memory, format: 'memory' }
];
const NODE_METRICS: MetricColumn[] = [
	CPU,
	{ label: '%CPU', index: -8, wide: true, value: (u) => percent(u.cpu, u.allocatable?.cpu), format: 'percent', threshold: 'cpu' },
	MEM,
	{ label: '%Mem', index: -9, wide: true, value: (u) => percent(u.memory, u.allocatable?.memory), format: 'percent', threshold: 'memory' },
	// The room left for new pods: what the node gives to pods minus what its pods request. Not
	// "free": that would read as not used, and these count requests, used or not.
	{ label: 'CPU left', index: -14, wide: false, value: (u) => free(u.allocatable?.cpu, u.requests?.cpu), format: 'cpu' },
	{ label: 'Mem left', index: -15, wide: false, value: (u) => free(u.allocatable?.memory, u.requests?.memory), format: 'memory' }
];

// What the pods of a namespace use and request, and how many of them are healthy.
const NAMESPACE_METRICS: MetricColumn[] = [
	{ label: 'Pods', index: -16, wide: false, value: (u) => u.pods?.total, format: 'pods' },
	CPU,
	MEM,
	{ label: 'CPU request', index: -10, wide: false, value: (u) => u.requests?.cpu, format: 'cpu' },
	{ label: 'Memory request', index: -12, wide: false, value: (u) => u.requests?.memory, format: 'memory' }
];

/** The metrics-server columns a kind's list shows, before its Age column. */
export function metricColumns(kind: MetricsKind): MetricColumn[] {
	return kind === 'nodes' ? NODE_METRICS : kind === 'namespaces' ? NAMESPACE_METRICS : POD_METRICS;
}

/** Empty cells, `<none>` and `None` all read as a dash. */
export const NOTHING = /^(|<none>|None)$/;

const TIMESTAMP = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)$/;

/** The index the Production column of namespaces sorts by. */
export const PRODUCTION = -20;

export type DisplayColumn = (
	| { kind: 'namespace'; label: string; index: -1 }
	| { kind: 'cell'; label: string; index: number; age: boolean; name: boolean; custom: boolean }
	| ({ kind: 'metric' } & MetricColumn)
	| { kind: 'production'; label: string; index: typeof PRODUCTION }
) & { wideOnly: boolean };

/**
 * What the cells of a list are read from. ResourceView passes getters, so a cell only redraws
 * when what it reads changes: an age every second, a usage when metrics come.
 */
export interface CellSource {
	resource: ResourceInfo;
	/** The columns of the table, as the cluster sends them. */
	columns: Column[];
	metricsKind: MetricsKind | null;
	usage(namespace: string | null, name: string): Usage | undefined;
	/** The last readings of a row's usage, for its sparkline. */
	trail(namespace: string | null, name: string): { cpu: number[]; memory: number[] } | undefined;
	now: number;
	/** When the snapshot on screen was taken, which ages count from. */
	taken: number | null;
	productionRuleOf(name: string, labels: Record<string, string>): ProductionRule | null;
	thresholds: K9sConfig['thresholds'] | null;
}

/** A column's title as the list shows it, short enough for a narrow column. */
export const shownLabel = (c: DisplayColumn) => c.label.replace(/^Memory/, 'Mem');
export const isNameColumn = (c: DisplayColumn) => c.kind === 'cell' && c.name;

/** A cell by its column's name, as text. */
export function cellOf(columns: Column[], row: Row, name: string): string | null {
	const index = columns.findIndex((c) => c.name === name);
	const value = index === -1 ? null : row.cells[index];
	return value === null || value === undefined ? null : String(value);
}

/** CPU or memory, for the columns of what pods and nodes use, which also show their limit or allocatable. */
export function usageColumn(s: CellSource, column: DisplayColumn): Measure | null {
	if (column.kind !== 'metric' || (s.metricsKind !== 'pods' && s.metricsKind !== 'nodes')) return null;
	return column.label === 'CPU' ? 'cpu' : column.label === 'Mem' ? 'memory' : null;
}

/** A namespace's pods: how many are healthy of all, as the overview counts them. */
function podsText(s: CellSource, row: Row): string {
	const pods = s.usage(row.namespace, row.name)?.pods;
	return pods ? `${pods.ok}/${pods.total}` : '—';
}

export function metricValue(s: CellSource, row: Row, column: MetricColumn) {
	const u = s.usage(row.namespace, row.name);
	return u ? column.value(u) : null;
}

/** A cell as the list shows it; `raw` keeps amounts as kubectl writes them, for the CSV. */
export function cellText(s: CellSource, row: Row, column: DisplayColumn, raw = false): string {
	switch (column.kind) {
		case 'namespace':
			return row.namespace ?? '';
		case 'production': {
			const rule = s.productionRuleOf(row.name, row.labels);
			return !rule ? '' : rule.by === 'name' ? t('Yes') : rule.rule;
		}
		case 'metric': {
			if (column.format === 'pods') return podsText(s, row);
			const value = metricValue(s, row, column);
			if (value === null || value === undefined) return '—';
			if (column.format === 'cpu') return raw ? cpu(value) : cpuText(value);
			return column.format === 'memory' ? (raw ? memory(value) : memoryText(value)) : `${value}%`;
		}
		case 'cell': {
			if (column.age) return age(row.created, s.taken ?? s.now);
			const value = row.cells[column.index];
			if (value === null || value === undefined) return '';
			if (column.label === 'Cluster-IP' && value === 'None') return 'Headless';
			if (column.label === 'Restarts' && typeof value === 'string' && !raw) return restarts(value);
			// A finished pod is not "0 of 1 ready": readiness no longer applies to it.
			if (!raw && /^ready$/i.test(column.label) && isPods(s.resource) && /^(Completed|Succeeded)$/.test(cellOf(s.columns, row, 'Status') ?? '')) return '—';
			if (column.label === 'Schedule' && s.resource.kind === 'CronJob') return describeCron(String(value));
			// Yes and No read better than True and False for switches such as Suspend. Condition
			// columns (Ready True) keep the word Kubernetes uses, and their color.
			if (typeof value === 'boolean' || (column.label === 'Suspend' && /^(true|false)$/i.test(String(value)))) {
				return String(value).toLowerCase() === 'true' ? t('Yes') : t('No');
			}
			// Times in custom columns, like a certificate's expiry, read as ages.
			if (column.custom && typeof value === 'string' && TIMESTAMP.test(value)) return relative(value, s.now);
			return String(value);
		}
	}
}

/** How close a pod is to its limit and request: red at 90 % of the limit, amber above the request. */
export function pressure(s: CellSource, row: Row, what: 'cpu' | 'memory'): { tone: Tone; hint: string } | null {
	if (!isPods(s.resource)) return null;
	const u = s.usage(row.namespace, row.name);
	if (!u) return null;
	const used = u[what];
	const limit = u.limits?.[what];
	const request = u.requests?.[what];
	if (limit && used >= limit * 0.9) {
		const n = Math.round((used / limit) * 100);
		return {
			tone: 'bad',
			hint:
				what === 'cpu'
					? t('At {n} % of its CPU limit: it is being throttled', { n })
					: t('At {n} % of its memory limit: it may be killed', { n })
		};
	}
	if (request && used > request) {
		const n = Math.round((used / request) * 100);
		return {
			tone: 'warn',
			hint: what === 'cpu' ? t('Using more CPU than it requests ({n} %)', { n }) : t('Using more memory than it requests ({n} %)', { n })
		};
	}
	return null;
}

/**
 * What a pod's status leaves out: that it runs at its CPU limit and is throttled, or close to
 * its memory limit and may be stopped. Shown as a word after the status.
 */
export function strains(s: CellSource, row: Row): { label: string; hint: string }[] {
	if (!isPods(s.resource)) return [];
	const out: { label: string; hint: string }[] = [];
	const cpuStrain = pressure(s, row, 'cpu');
	if (cpuStrain?.tone === 'bad') out.push({ label: t('throttled'), hint: cpuStrain.hint });
	const memoryStrain = pressure(s, row, 'memory');
	if (memoryStrain?.tone === 'bad') out.push({ label: t('near memory limit'), hint: memoryStrain.hint });
	return out;
}

/** What a column's title means, where its name alone does not say. */
export function headerHint(column: DisplayColumn): string | undefined {
	if (column.kind !== 'metric') return undefined;
	if (column.label === 'CPU left' || column.label === 'Mem left') {
		return t('What the node can still give to new pods: what it has for pods, minus what its pods request, used or not');
	}
	if (column.format === 'cpu' && !units.kubectl) return t('In cores');
	return undefined;
}

/** Colors status cells, and percentages by the k9s thresholds. */
export function tone(s: CellSource, row: Row, column: DisplayColumn, text: string): Tone {
	const status = () => cellOf(s.columns, row, 'Status') ?? '';
	// A pod that finished is not "0 of 1 ready"; that is how it ends.
	if (column.kind === 'cell' && /^ready$/i.test(column.label) && /^(Completed|Succeeded)$/.test(status())) {
		return '';
	}
	// A pod's readiness takes the color of its status, so a crashing pod is red in both.
	if (column.kind === 'cell' && /^ready$/i.test(column.label) && isPods(s.resource)) {
		const ready = cellTone(column.label, text);
		if (ready === 'ok' || ready === '') return ready;
		return cellTone('Status', status()) === 'bad' ? 'bad' : 'warn';
	}
	if (column.kind === 'cell') return cellTone(column.label, text);
	if (column.kind === 'metric' && (column.label === 'CPU' || column.label === 'Mem')) {
		return pressure(s, row, column.label === 'CPU' ? 'cpu' : 'memory')?.tone ?? '';
	}
	if (column.kind === 'metric' && column.format === 'pods') {
		const pods = s.usage(row.namespace, row.name)?.pods;
		return !pods || !pods.total ? '' : pods.failed ? 'bad' : pods.warning ? 'warn' : 'ok';
	}
	if (column.kind !== 'metric' || !column.threshold) return '';
	if (column.label.endsWith('/ request')) {
		const v = metricValue(s, row, column);
		return v !== null && v !== undefined && v > 100 ? 'warn' : '';
	}
	const value = metricValue(s, row, column);
	if (value === null || value === undefined) return '';
	const limits = s.thresholds?.[column.threshold] ?? { warn: 70, critical: 90 };
	return value >= limits.critical ? 'bad' : value >= limits.warn ? 'warn' : '';
}

/** An icon next to status words, so that the state does not hang on color alone. */
export function statusIcon(column: DisplayColumn, tone: Tone): IconName | null {
	if (column.kind !== 'cell' || !/^(status|phase|state)$/i.test(column.label)) return null;
	return tone === 'ok' ? 'ok' : tone === 'warn' ? 'pending' : tone === 'bad' ? 'critical' : null;
}

/**
 * The columns that fit in `space` pixels, the least important left out first, and how wide a
 * name may be: a long name is cut short before other columns go; its whole name is in the tooltip.
 */
export function fitColumns(s: CellSource, columns: DisplayColumn[], rows: Row[], space: number) {
	const nameMax = Math.max(200, Math.round(space * 0.4));
	const sample = rows.slice(0, 200);
	const fit = columns.map((c) => {
		const rank = columnRank(c.label, c.kind, isNameColumn(c));
		const measure = usageColumn(s, c);
		const texts = sample.map((r) => {
			const suffix = measure ? limitSuffix(s.usage(r.namespace, r.name), measure) : null;
			return suffix ? `${cellText(s, r, c)} ${suffix}` : cellText(s, r, c);
		});
		const width = columnWidth(shownLabel(c), texts, { mono: isNameColumn(c), icon: rank === Infinity && !isNameColumn(c), max: isNameColumn(c) ? nameMax : undefined });
		return { label: c.label, rank, width };
	});
	const left = overflowing(fit, space);
	return { drawn: columns.filter((c) => !left.includes(c.label)), left, nameMax };
}

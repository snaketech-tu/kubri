import type { Amounts, Capacity, ContainerResources, KubeObject } from './api';
import { cpu as formatCpu, memory as formatMemory } from './format';
import { t } from './i18n';

/** What a request or limit is of. */
export type Measure = 'cpu' | 'memory';
export const MEASURES: Measure[] = ['cpu', 'memory'];

type Pair = Record<Measure, string>;

/** One container's requests and limits: as typed, and as they are on the cluster. */
export interface ResourceRow {
	container: string;
	requests: Pair;
	limits: Pair;
	was: { requests: Pair; limits: Pair };
}

const SUFFIXES: Record<string, number> = {
	'': 1,
	n: 1e-9,
	u: 1e-6,
	m: 1e-3,
	k: 1e3,
	M: 1e6,
	G: 1e9,
	T: 1e12,
	P: 1e15,
	E: 1e18,
	Ki: 1024,
	Mi: 1024 ** 2,
	Gi: 1024 ** 3,
	Ti: 1024 ** 4,
	Pi: 1024 ** 5,
	Ei: 1024 ** 6
};

/** A Kubernetes quantity as a number, in cores or bytes: `250m` is 0.25, `1Gi` 1073741824. */
export function parseQuantity(text: string): number | null {
	const m = /^\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)\s*([a-zA-Z]*)\s*$/.exec(text);
	if (!m || !(m[2] in SUFFIXES)) return null;
	return Number(m[1]) * SUFFIXES[m[2]];
}

/** An amount for people: CPU as `250m` or `1.5`, memory as `512Mi` or `1.5Gi`. */
export function formatAmount(measure: Measure, value: number): string {
	if (measure === 'memory') return formatMemory(value);
	if (value >= 1) return `${Math.round(value * 100) / 100}`;
	return formatCpu(value * 1000);
}

/** Common sizes, which the arrow keys step through. */
const LADDER: Record<Measure, string[]> = {
	cpu: ['10m', '25m', '50m', '100m', '150m', '200m', '250m', '300m', '400m', '500m', '750m', '1', '1.5', '2', '3', '4', '6', '8', '12', '16'],
	memory: [
		'16Mi', '32Mi', '64Mi', '128Mi', '192Mi', '256Mi', '384Mi', '512Mi', '768Mi',
		'1Gi', '1.5Gi', '2Gi', '3Gi', '4Gi', '6Gi', '8Gi', '12Gi', '16Gi', '24Gi', '32Gi'
	]
};
/** Where an empty field starts. */
const START: Record<Measure, string> = { cpu: '100m', memory: '128Mi' };

/** The sizes to suggest for a field. */
export function ladder(measure: Measure): string[] {
	return LADDER[measure];
}

/**
 * The next common size above (`by` 1) or below (`by` -1) `value`. An empty field starts at a
 * small size; text that is no amount stays as it is.
 */
export function step(measure: Measure, value: string, by: 1 | -1): string {
	if (!value.trim()) return START[measure];
	const now = parseQuantity(value);
	if (now === null) return value;
	const sizes = LADDER[measure].map((s) => [s, parseQuantity(s)!] as const);
	// Slightly off, so 1000m counts as 1 and steps past it.
	const near = now * 1e-6;
	if (by > 0) return sizes.find(([, q]) => q > now + near)?.[0] ?? formatAmount(measure, now * 2);
	return sizes.findLast(([, q]) => q < now - near)?.[0] ?? sizes[0][0];
}

interface Container {
	name: string;
	resources?: { requests?: Record<string, string>; limits?: Record<string, string> };
}

/** The pod template's spec, or a pod's own. */
function podSpec(object: KubeObject): { containers?: Container[]; initContainers?: Container[] } {
	const spec = object.spec as { template?: { spec?: object }; containers?: Container[] } | undefined;
	return (spec?.template?.spec ?? spec ?? {}) as { containers?: Container[]; initContainers?: Container[] };
}

/** A row for each of the object's containers. */
export function rowsOf(object: KubeObject): ResourceRow[] {
	return (podSpec(object).containers ?? []).map((c) => {
		const pair = (values?: Record<string, string>): Pair => ({ cpu: values?.cpu ?? '', memory: values?.memory ?? '' });
		const requests = pair(c.resources?.requests);
		const limits = pair(c.resources?.limits);
		return { container: c.name, requests: { ...requests }, limits: { ...limits }, was: { requests, limits } };
	});
}

/** What init containers request at most, which a pod needs while they run. */
export function initRequests(object: KubeObject): Amounts {
	const most = { cpu: 0, memory: 0 };
	for (const c of podSpec(object).initContainers ?? []) {
		for (const m of MEASURES) {
			const q = parseQuantity(c.resources?.requests?.[m] ?? c.resources?.limits?.[m] ?? '') ?? 0;
			most[m] = Math.max(most[m], q);
		}
	}
	return most;
}

/** The containers whose requests or limits change: an emptied field removes the value. */
export function resourceChanges(rows: ResourceRow[]): ContainerResources[] {
	const out: ContainerResources[] = [];
	for (const row of rows) {
		const change: ContainerResources = { container: row.container, requests: {}, limits: {} };
		for (const part of ['requests', 'limits'] as const) {
			for (const m of MEASURES) {
				const now = row[part][m].trim();
				const was = row.was[part][m];
				if (now === was) continue;
				if (now) change[part][m] = now;
				else if (was) change[part][m] = null;
			}
		}
		if (Object.keys(change.requests).length || Object.keys(change.limits).length) out.push(change);
	}
	return out;
}

/** A field that cannot be set as typed. */
export interface FieldProblem {
	container: string;
	part: 'requests' | 'limits';
	measure: Measure;
	text: string;
}

/** What is wrong with the fields: amounts Kubernetes does not take, limits below requests. */
export function fieldProblems(rows: ResourceRow[]): FieldProblem[] {
	const out: FieldProblem[] = [];
	for (const row of rows) {
		for (const m of MEASURES) {
			const amounts: Partial<Record<'requests' | 'limits', number>> = {};
			for (const part of ['requests', 'limits'] as const) {
				const text = row[part][m].trim();
				if (!text) continue;
				const q = parseQuantity(text);
				if (q === null || q <= 0) {
					out.push({
						container: row.container,
						part,
						measure: m,
						text: m === 'cpu' ? t('Not an amount of CPU, such as 250m or 1') : t('Not an amount of memory, such as 256Mi or 1Gi')
					});
				} else amounts[part] = q;
			}
			if (amounts.requests !== undefined && amounts.limits !== undefined && amounts.limits < amounts.requests) {
				out.push({ container: row.container, part: 'limits', measure: m, text: t('The limit is below the request') });
			}
		}
	}
	return out;
}

/** What a container requests: its request, or its limit where it has none, as Kubernetes does. */
function requested(row: ResourceRow, measure: Measure, before: boolean): number {
	const values = before ? row.was : row;
	return parseQuantity(values.requests[measure] || values.limits[measure] || '') ?? 0;
}

/** What one pod requests, before or after the change. */
export function podTotals(rows: ResourceRow[], before: boolean, init: Amounts = { cpu: 0, memory: 0 }): Amounts {
	const sum = { cpu: 0, memory: 0 };
	for (const row of rows) for (const m of MEASURES) sum[m] += requested(row, m, before);
	return { cpu: Math.max(sum.cpu, init.cpu), memory: Math.max(sum.memory, init.memory) };
}

/** The limits of one pod; null for a measure where a container has none. */
export function podLimits(rows: ResourceRow[], before: boolean): Record<Measure, number | null> {
	const out: Record<Measure, number | null> = { cpu: 0, memory: 0 };
	for (const row of rows) {
		const values = before ? row.was : row;
		for (const m of MEASURES) {
			const q = parseQuantity(values.limits[m]);
			out[m] = q === null || out[m] === null ? null : out[m]! + q;
		}
	}
	return out;
}

/** CPU or memory on the nodes the pods may run on, before and after the change. */
export interface MeasureBudget {
	measure: Measure;
	allocatable: number;
	/** Requested by other pods. */
	others: number;
	/** Requested by the workload's pods at their size now, and at the new size. */
	before: number;
	after: number;
}

/** The budget the dialog shows, and what it warns about. */
export interface Budget {
	nodes: number;
	measures: MeasureBudget[];
	/** How many pods of the new size fit, with the workload's old pods gone. */
	placeable: number;
	warnings: { level: 'bad' | 'warn'; text: string }[];
}

/** How many pods of `size` fit into `free`. */
function fitCount(free: Amounts, size: Amounts): number {
	let n = Infinity;
	for (const m of MEASURES) if (size[m] > 0) n = Math.min(n, Math.floor(Math.max(0, free[m]) / size[m] + 1e-9));
	return n;
}

/**
 * The room for the workload's pods at their new size on the nodes they may run on, and in the
 * namespace's quotas. `daemon` is for a DaemonSet, whose pods run one per node.
 */
export function budget(capacity: Capacity, before: Amounts, after: Amounts, limits: { before: Record<Measure, number | null>; after: Record<Measure, number | null> }, daemon = false): Budget {
	const warnings: Budget['warnings'] = [];
	const pods = capacity.pods;
	const eligible = (capacity.nodes ?? []).filter((n) => n.eligible);
	const measures = MEASURES.map((m) => {
		const allocatable = eligible.reduce((s, n) => s + n.allocatable[m], 0);
		const requested = eligible.reduce((s, n) => s + n.requested[m], 0);
		const own = eligible.reduce((s, n) => s + n.own[m], 0);
		// Old pods still around during a rollout count as others; `before` is what the template asks.
		return { measure: m, allocatable, others: Math.max(0, requested - own), before: before[m] * pods, after: after[m] * pods };
	});
	let placeable = 0;
	let beside = 0;
	let missing = 0;
	for (const n of eligible) {
		const free = { cpu: n.allocatable.cpu - (n.requested.cpu - n.own.cpu), memory: n.allocatable.memory - (n.requested.memory - n.own.memory) };
		const fits = fitCount(free, after);
		placeable += daemon ? Math.min(1, fits) : fits;
		if (daemon && fits < 1) missing++;
		beside += fitCount({ cpu: n.allocatable.cpu - n.requested.cpu, memory: n.allocatable.memory - n.requested.memory }, after);
	}
	const grows = after.cpu > before.cpu || after.memory > before.memory;
	if (capacity.nodes && pods > 0) {
		if (!eligible.length) warnings.push({ level: 'bad', text: t('No node takes these pods, whatever their size') });
		else if (daemon && missing) {
			warnings.push({
				level: 'bad',
				text: t('{n} of the nodes have no room for a pod of this size', { n: missing })
			});
		} else if (placeable === 0) warnings.push({ level: 'bad', text: t('No node has room for a pod of this size') });
		else if (placeable < pods) {
			warnings.push({
				level: 'bad',
				text: t('Only {fit} of {pods} pods of this size fit on the nodes; the rest stay pending', { fit: placeable, pods })
			});
		} else if (grows && !daemon && beside === 0) {
			warnings.push({
				level: 'warn',
				text: t('A new pod only fits once an old one is gone; with one replica or no room to surge, the rollout waits')
			});
		}
		for (const b of measures) {
			if (b.allocatable > 0 && b.others + b.after > 0.9 * b.allocatable && b.others + b.after <= b.allocatable && grows) {
				warnings.push({
					level: 'warn',
					text: b.measure === 'cpu' ? t('More than 90% of the CPU on these nodes would be requested') : t('More than 90% of the memory on these nodes would be requested')
				});
			}
		}
	}
	for (const q of capacity.quotas) {
		const [part, m] = q.resource.split('.') as ['requests' | 'limits', Measure];
		const per = part === 'requests' ? { b: before[m], a: after[m] } : { b: limits.before[m], a: limits.after[m] };
		if (per.a === null) {
			warnings.push({ level: 'bad', text: t('The quota {name} needs a limit on {resource} for every container', { name: q.name, resource: m }) });
			continue;
		}
		const used = q.used - (per.b ?? 0) * pods + per.a * pods;
		if (used > q.hard + 1e-9 && used > q.used) {
			warnings.push({
				level: 'bad',
				text: t('The quota {name} allows {hard} of {resource}; this needs {used}', {
					name: q.name,
					resource: q.resource,
					hard: formatAmount(m, q.hard),
					used: formatAmount(m, used)
				})
			});
		}
	}
	return { nodes: eligible.length, measures, placeable, warnings };
}

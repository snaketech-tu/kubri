import type { Health, ResourceInfo, XrayNode } from './api';

/** The kinds XRay can start from, in the order the picker shows them. */
const ROOTS: [group: string, kind: string][] = [
	['apps', 'Deployment'],
	['apps', 'StatefulSet'],
	['apps', 'DaemonSet'],
	['apps', 'ReplicaSet'],
	['batch', 'Job'],
	['batch', 'CronJob'],
	['', 'Pod'],
	['', 'Service'],
	['networking.k8s.io', 'Ingress']
];

export function xrayResources(resources: ResourceInfo[]): ResourceInfo[] {
	return ROOTS.map(([group, kind]) => resources.find((r) => r.group === group && r.kind === kind)).filter(
		(r): r is ResourceInfo => !!r
	);
}

export function canXray(resource: ResourceInfo) {
	return ROOTS.some(([group, kind]) => resource.group === group && resource.kind === kind);
}

const SHORT: Record<string, string> = {
	Deployment: 'deploy',
	StatefulSet: 'sts',
	DaemonSet: 'ds',
	ReplicaSet: 'rs',
	Job: 'job',
	CronJob: 'cron',
	Pod: 'pod',
	Service: 'svc',
	Ingress: 'ing',
	Container: 'co',
	InitContainer: 'init',
	ConfigMap: 'cm',
	Secret: 'sec',
	ServiceAccount: 'sa',
	PersistentVolumeClaim: 'pvc'
};

/** A short tag for a tree row, like the k9s aliases. */
export function kindTag(kind: string) {
	return SHORT[kind] ?? kind.toLowerCase();
}

/** How many nodes below `node` are not ok. */
export function issuesBelow(node: XrayNode): number {
	return node.children.reduce((sum, c) => sum + (c.health === 'ok' ? 0 : 1) + issuesBelow(c), 0);
}

function hasIssue(node: XrayNode): boolean {
	return node.health !== 'ok' || node.children.some(hasIssue);
}

/** What starts expanded: workloads down to their pods, and every path to a problem. */
export function initialExpanded(roots: XrayNode[]): Set<string> {
	const open = new Set<string>();
	const visit = (node: XrayNode, depth: number) => {
		if (!node.children.length) return;
		const leadsToProblem = node.children.some(hasIssue);
		if (leadsToProblem || (depth < 2 && node.kind !== 'Pod')) open.add(node.id);
		for (const c of node.children) visit(c, depth + 1);
	};
	for (const r of roots) visit(r, 0);
	return open;
}

export function allIds(roots: XrayNode[]): Set<string> {
	const ids = new Set<string>();
	const visit = (node: XrayNode) => {
		if (node.children.length) ids.add(node.id);
		node.children.forEach(visit);
	};
	roots.forEach(visit);
	return ids;
}

export interface TreeRow {
	node: XrayNode;
	depth: number;
	parent: string | null;
	expanded: boolean;
}

export interface TreeFilter {
	text: string;
	/** Keep only paths that lead to a warning or failure. */
	issuesOnly: boolean;
}

function matchesText(node: XrayNode, text: string) {
	return [node.name, node.kind, node.detail, node.namespace ?? ''].some((v) => v.toLowerCase().includes(text));
}

/**
 * The rows to show. A filter keeps the matching nodes with their ancestors, and opens those
 * ancestors so the matches are visible.
 */
export function visibleRows(roots: XrayNode[], expanded: Set<string>, filter: TreeFilter): TreeRow[] {
	const text = filter.text.trim().toLowerCase();
	const filtering = text !== '' || filter.issuesOnly;
	const keep = (node: XrayNode): boolean =>
		((!text || matchesText(node, text)) && (!filter.issuesOnly || node.health !== 'ok')) || node.children.some(keep);
	const rows: TreeRow[] = [];
	const visit = (node: XrayNode, depth: number, parent: string | null) => {
		if (filtering && !keep(node)) return;
		const open = expanded.has(node.id) || (filtering && node.children.some(keep));
		rows.push({
			node,
			depth,
			parent,
			expanded: open && node.children.length > 0
		});
		if (open) for (const c of node.children) visit(c, depth + 1, node.id);
	};
	for (const r of roots) visit(r, 0, null);
	return rows;
}

const HEALTH_ORDER: Record<Health, number> = { ok: 0, warning: 1, failed: 2 };

export function worst(a: Health, b: Health): Health {
	return HEALTH_ORDER[a] >= HEALTH_ORDER[b] ? a : b;
}

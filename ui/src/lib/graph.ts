import type { Graph, GraphEdge, GraphNode } from './api';

export type GroupBy = 'namespace' | 'app' | 'node';

/** Where each kind goes, left to right: traffic comes in on the left, nodes are on the right. */
const COLUMN: Record<string, number> = {
	Ingress: 0,
	Service: 1,
	Deployment: 2,
	StatefulSet: 2,
	DaemonSet: 2,
	CronJob: 2,
	Job: 2,
	ReplicaSet: 2,
	// Under the workloads that use them, in the same column.
	ConfigMap: 2,
	Secret: 2,
	PersistentVolumeClaim: 2,
	Pod: 3,
	Node: 4
};
const CONFIG = new Set(['ConfigMap', 'Secret', 'PersistentVolumeClaim']);

export const BOX = { width: 196, height: 42 };
const COLUMN_GAP = 84;
const ROW = 54;
const BAND_GAP = 28;
const BAND_TOP = 34;
const PAD = 16;

export interface Box {
	node: GraphNode;
	x: number;
	y: number;
}

export interface Band {
	label: string;
	x: number;
	y: number;
	width: number;
	height: number;
	/** How many objects in it are not healthy. */
	issues: number;
}

export interface Layout {
	boxes: Box[];
	bands: Band[];
	edges: { edge: GraphEdge; d: string }[];
	width: number;
	height: number;
}

function groupOf(node: GraphNode, by: GroupBy): string | null {
	switch (by) {
		case 'namespace':
			return node.namespace;
		case 'app':
			return node.app;
		case 'node':
			return node.node;
	}
}

/** The objects shown: nodes only when grouping by them, where each band ends in its node. */
export function shownNodes(graph: Graph, by: GroupBy): GraphNode[] {
	return graph.nodes.filter((n) => n.kind !== 'Node' || by === 'node');
}

function byName(a: GraphNode, b: GraphNode) {
	return a.name.localeCompare(b.name) || a.id.localeCompare(b.id);
}

/**
 * Lays the graph out in bands, one per namespace, application or node, each with columns from
 * Ingresses to nodes. Within a column objects follow what they connect to on the left, so edges
 * cross as little as a single pass allows.
 */
export function layoutGraph(graph: Graph, by: GroupBy): Layout {
	const nodes = shownNodes(graph, by);
	const shown = new Set(nodes.map((n) => n.id));
	// A Service that selects the pods of a workload points at the workload, not at each pod.
	const owner = new Map(graph.edges.filter((e) => e.kind === 'owns').map((e) => [e.to, e.from]));
	const seen = new Set<string>();
	const edges = graph.edges
		.map((e) => (e.kind === 'selects' && owner.has(e.to) ? { ...e, to: owner.get(e.to)! } : e))
		.filter((e) => shown.has(e.from) && shown.has(e.to))
		.filter((e) => {
			const key = `${e.from}>${e.to}`;
			if (seen.has(key)) return false;
			seen.add(key);
			return true;
		});
	const neighbors = new Map<string, string[]>();
	for (const e of edges) {
		neighbors.set(e.from, [...(neighbors.get(e.from) ?? []), e.to]);
		neighbors.set(e.to, [...(neighbors.get(e.to) ?? []), e.from]);
	}

	// Objects without a group of their own, like a Service when grouping by node, join the
	// group of what they connect to.
	const fallback = by === 'namespace' ? 'Cluster' : by === 'app' ? 'Other' : 'Not on a node';
	const group = new Map(nodes.map((n) => [n.id, groupOf(n, by) ?? fallback]));
	for (let round = 0; round < 3; round++) {
		for (const n of nodes) {
			if (group.get(n.id) !== fallback) continue;
			const near = (neighbors.get(n.id) ?? []).map((id) => group.get(id)).find((g) => g && g !== fallback);
			if (near) group.set(n.id, near);
		}
	}
	const groups = new Map<string, GraphNode[]>();
	for (const n of nodes) {
		const key = group.get(n.id) ?? fallback;
		groups.set(key, [...(groups.get(key) ?? []), n]);
	}
	// Bands with problems first, so they are on screen without scrolling; then by name.
	const issuesIn = (label: string) => (groups.get(label) ?? []).filter((n) => n.health !== 'ok' && n.kind !== 'Node').length;
	const order = [...groups.keys()].sort(
		(a, b) =>
			Number(a === fallback) - Number(b === fallback) || Number(issuesIn(b) > 0) - Number(issuesIn(a) > 0) || a.localeCompare(b)
	);

	const boxes: Box[] = [];
	const bands: Band[] = [];
	const width = PAD * 2 + 5 * BOX.width + 4 * COLUMN_GAP;
	let top = PAD;
	for (const label of order) {
		const members = groups.get(label) ?? [];
		// Rows within this band; what connects across bands does not pull.
		const position = new Map<string, number>();
		const columns: GraphNode[][] = [[], [], [], [], []];
		for (const n of members) columns[COLUMN[n.kind] ?? 2].push(n);
		// The mean place of what an object connects to among those placed already.
		const center = (n: GraphNode) => {
			const placed = (neighbors.get(n.id) ?? []).map((id) => position.get(id)).filter((p): p is number => p !== undefined);
			return placed.length ? placed.reduce((a, b) => a + b, 0) / placed.length : Infinity;
		};
		const place = (list: GraphNode[], start = 0) =>
			list.forEach((n, i) => {
				position.set(n.id, start + i);
			});
		const sortBy = (list: GraphNode[], key: (n: GraphNode) => number) =>
			list.sort((a, b) => {
				const ka = key(a);
				const kb = key(b);
				return ka === kb ? byName(a, b) : ka - kb;
			});

		columns[0].sort(byName);
		place(columns[0]);
		sortBy(columns[1], center);
		place(columns[1]);
		// Workloads follow the Services of their pods; config follows the workloads using it.
		const workloads = columns[2].filter((n) => !CONFIG.has(n.kind));
		const config = columns[2].filter((n) => CONFIG.has(n.kind));
		const viaPods = (n: GraphNode) => {
			const services = (neighbors.get(n.id) ?? [])
				.flatMap((pod) => neighbors.get(pod) ?? [])
				.map((id) => (id.startsWith('Service/') ? position.get(id) : undefined))
				.filter((p): p is number => p !== undefined);
			return services.length ? services.reduce((a, b) => a + b, 0) / services.length : center(n);
		};
		sortBy(workloads, (n) => {
			const direct = center(n);
			return Number.isFinite(direct) ? direct : viaPods(n);
		});
		place(workloads);
		sortBy(config, center);
		place(config, workloads.length);
		columns[2] = [...workloads, ...config];
		sortBy(columns[3], center);
		place(columns[3]);
		sortBy(columns[4], center);
		place(columns[4]);

		const rows = Math.max(1, ...columns.map((c) => c.length));
		const height = BAND_TOP + rows * ROW;
		columns.forEach((column, c) => {
			// Short columns sit at the top, next to what they connect to.
			column.forEach((node, r) =>
				boxes.push({ node, x: PAD + c * (BOX.width + COLUMN_GAP), y: top + BAND_TOP + r * ROW })
			);
		});
		bands.push({
			label,
			x: PAD / 2,
			y: top,
			width: width - PAD,
			height,
			issues: members.filter((n) => n.health !== 'ok').length
		});
		top += height + BAND_GAP;
	}

	const at = new Map(boxes.map((b) => [b.node.id, b]));
	const paths = edges.flatMap((edge) => {
		const a = at.get(edge.from);
		const b = at.get(edge.to);
		return a && b ? [{ edge, d: curve(a, b) }] : [];
	});
	return { boxes, bands, edges: paths, width, height: Math.max(top - BAND_GAP + PAD, 120) };
}

/** A curve from the right of `a` to the left of `b`; within a column, a loop on the right. */
export function curve(a: { x: number; y: number }, b: { x: number; y: number }): string {
	const y1 = a.y + BOX.height / 2;
	const y2 = b.y + BOX.height / 2;
	if (a.x === b.x) {
		const x = a.x + BOX.width;
		const bend = Math.min(60, 18 + Math.abs(y2 - y1) / 4);
		return `M${x},${y1} C${x + bend},${y1} ${x + bend},${y2} ${x},${y2}`;
	}
	const [from, to] = a.x < b.x ? [a, b] : [b, a];
	const [fy, ty] = a.x < b.x ? [y1, y2] : [y2, y1];
	const x1 = from.x + BOX.width;
	const x2 = to.x;
	const mid = (x1 + x2) / 2;
	return `M${x1},${fy} C${mid},${fy} ${mid},${ty} ${x2},${ty}`;
}

/** The objects connected to `id`, and `id` itself. */
export function neighborhood(edges: GraphEdge[], id: string): Set<string> {
	const near = new Set([id]);
	for (const e of edges) {
		if (e.from === id) near.add(e.to);
		if (e.to === id) near.add(e.from);
	}
	return near;
}

/** Whether a node matches a filter: part of its name, kind or detail. */
export function matches(node: GraphNode, text: string): boolean {
	const t = text.trim().toLowerCase();
	return !t || node.name.toLowerCase().includes(t) || node.kind.toLowerCase().includes(t) || node.detail.toLowerCase().includes(t);
}

/**
 * The scale and offset that fit `layout` into a view of `width` by `height`; with `across`, only
 * its width, so a tall graph stays readable and is panned down.
 */
export function fit(layout: Pick<Layout, 'width' | 'height'>, width: number, height: number, across = false) {
	const whole = Math.min(width / layout.width, height / layout.height);
	const scale = Math.min(1.1, Math.max(across ? 0.45 : 0.15, across ? width / layout.width : whole));
	return { scale, x: Math.max(0, (width - layout.width * scale) / 2), y: 0 };
}

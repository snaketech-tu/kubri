import { describe, expect, it } from 'vitest';
import type { Graph, GraphNode } from './api';
import { BOX, curve, layoutGraph, matches, neighborhood } from './graph';

const node = (kind: string, name: string, extra: Partial<GraphNode> = {}): GraphNode => ({
	id: `${kind}/shop/${name}`,
	kind,
	name,
	namespace: kind === 'Node' ? null : 'shop',
	resource: { group: '', version: 'v1', plural: `${kind.toLowerCase()}s` },
	health: 'ok',
	detail: '',
	app: 'web',
	node: 'node-a',
	...extra
});

const graph: Graph = {
	nodes: [
		node('Ingress', 'shop', { node: null }),
		node('Service', 'web', { node: null }),
		node('Service', 'api', { app: 'api' }),
		node('Deployment', 'web'),
		node('Secret', 'web-env'),
		node('Pod', 'web-1'),
		node('Pod', 'web-2', { health: 'failed', node: 'node-b' }),
		node('Node', 'node-a', { id: 'Node//node-a', app: null })
	],
	edges: [
		{ from: 'Ingress/shop/shop', to: 'Service/shop/web', kind: 'routes' },
		{ from: 'Service/shop/web', to: 'Pod/shop/web-1', kind: 'selects' },
		{ from: 'Service/shop/web', to: 'Pod/shop/web-2', kind: 'selects' },
		{ from: 'Deployment/shop/web', to: 'Pod/shop/web-1', kind: 'owns' },
		{ from: 'Deployment/shop/web', to: 'Pod/shop/web-2', kind: 'owns' },
		{ from: 'Deployment/shop/web', to: 'Secret/shop/web-env', kind: 'uses' },
		{ from: 'Pod/shop/web-1', to: 'Node//node-a', kind: 'runsOn' }
	],
	paths: [],
	missing: []
};

describe('layoutGraph', () => {
	it('puts kinds in columns from Ingress to Pod, one band per application', () => {
		const layout = layoutGraph(graph, 'app');
		// Bands with problems come first: web has a failing pod.
		expect(layout.bands.map((b) => b.label)).toEqual(['web', 'api']);
		const at = (id: string) => layout.boxes.find((b) => b.node.id === id)!;
		expect(at('Ingress/shop/shop').x).toBeLessThan(at('Service/shop/web').x);
		expect(at('Service/shop/web').x).toBeLessThan(at('Deployment/shop/web').x);
		expect(at('Deployment/shop/web').x).toBeLessThan(at('Pod/shop/web-1').x);
		// Config sits under the workloads, in their column.
		expect(at('Secret/shop/web-env').x).toBe(at('Deployment/shop/web').x);
		expect(at('Secret/shop/web-env').y).toBeGreaterThan(at('Deployment/shop/web').y);
		// Nodes only show when grouping by them.
		expect(at('Node//node-a')).toBeUndefined();
		expect(layout.bands.find((b) => b.label === 'web')?.issues).toBe(1);
		// The Service points at the Deployment of its pods, once.
		expect(layout.edges.filter((e) => e.edge.kind === 'selects').map((e) => e.edge.to)).toEqual(['Deployment/shop/web']);
		expect(layout.edges).toHaveLength(5);
	});

	it('groups by node, with each node at the end of its band', () => {
		const layout = layoutGraph(graph, 'node');
		expect(layout.bands.map((b) => b.label)).toEqual(['node-b', 'node-a']);
		// The Ingress has no node of its own and joins that of its Service's pods.
		const band = (id: string) => layout.bands.find((b) => {
			const box = layout.boxes.find((x) => x.node.id === id)!;
			return box.y >= b.y && box.y < b.y + b.height;
		})?.label;
		expect(band('Ingress/shop/shop')).toBe('node-a');
		expect(layout.boxes.find((b) => b.node.id === 'Node//node-a')).toBeDefined();
		expect(layout.edges.some((e) => e.edge.kind === 'runsOn')).toBe(true);
	});

	it('draws curves between columns and loops within one', () => {
		expect(curve({ x: 0, y: 0 }, { x: 300, y: 0 })).toBe(`M${BOX.width},21 C${(BOX.width + 300) / 2},21 ${(BOX.width + 300) / 2},21 300,21`);
		expect(curve({ x: 0, y: 0 }, { x: 0, y: 100 })).toMatch(/^M196,21 C/);
	});
});

describe('graph helpers', () => {
	it('finds neighbors and matches filters', () => {
		expect([...neighborhood(graph.edges, 'Service/shop/web')].sort()).toEqual(
			['Ingress/shop/shop', 'Pod/shop/web-1', 'Pod/shop/web-2', 'Service/shop/web'].sort()
		);
		expect(matches(graph.nodes[1], 'serv')).toBe(true);
		expect(matches(graph.nodes[1], 'nope')).toBe(false);
	});
});

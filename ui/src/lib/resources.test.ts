import { describe, expect, it } from 'vitest';
import type { Capacity, KubeObject, NodeRoom } from './api';
import { budget, fieldProblems, formatAmount, initRequests, parseQuantity, podLimits, podTotals, resourceChanges, rowsOf, step } from './resources';

const GI = 1024 ** 3;
const MI = 1024 ** 2;

function deployment(containers: object[], initContainers: object[] = []): KubeObject {
	return {
		apiVersion: 'apps/v1',
		kind: 'Deployment',
		metadata: { name: 'web', namespace: 'shop' },
		spec: { template: { spec: { containers, initContainers } } }
	} as unknown as KubeObject;
}

const web = deployment([
	{ name: 'web', resources: { requests: { cpu: '100m', memory: '128Mi' }, limits: { cpu: '500m', memory: '256Mi' } } },
	{ name: 'sidecar' }
]);

function node(name: string, over: Partial<NodeRoom> = {}): NodeRoom {
	return {
		name,
		eligible: true,
		reasons: [],
		allocatable: { cpu: 2, memory: 4 * GI },
		requested: { cpu: 0, memory: 0 },
		own: { cpu: 0, memory: 0 },
		ownPods: 0,
		...over
	};
}

describe('quantities', () => {
	it('reads and writes amounts', () => {
		expect(parseQuantity('250m')).toBe(0.25);
		expect(parseQuantity('1.5')).toBe(1.5);
		expect(parseQuantity('1Gi')).toBe(GI);
		expect(parseQuantity('2e3')).toBe(2000);
		expect(parseQuantity('12 apples')).toBeNull();
		expect(parseQuantity('')).toBeNull();
		expect(formatAmount('cpu', 0.25)).toBe('250m');
		expect(formatAmount('cpu', 1.5)).toBe('1.5');
		expect(formatAmount('memory', 512 * MI)).toBe('512Mi');
	});

	it('steps through common sizes', () => {
		expect(step('cpu', '', 1)).toBe('100m');
		expect(step('cpu', '100m', 1)).toBe('150m');
		expect(step('cpu', '120m', 1)).toBe('150m');
		expect(step('cpu', '120m', -1)).toBe('100m');
		expect(step('cpu', '1000m', 1)).toBe('1.5');
		expect(step('cpu', '10m', -1)).toBe('10m');
		expect(step('cpu', '16', 1)).toBe('32');
		expect(step('memory', '1Gi', -1)).toBe('768Mi');
		expect(step('memory', '128Mi', 1)).toBe('192Mi');
		expect(step('memory', 'lots', 1)).toBe('lots');
	});
});

describe('rows', () => {
	it('reads every container', () => {
		const rows = rowsOf(web);
		expect(rows.map((r) => r.container)).toEqual(['web', 'sidecar']);
		expect(rows[0].limits).toEqual({ cpu: '500m', memory: '256Mi' });
		expect(rows[1].requests).toEqual({ cpu: '', memory: '' });
	});

	it('sends only what changes, and empties as removals', () => {
		const rows = rowsOf(web);
		expect(resourceChanges(rows)).toEqual([]);
		rows[0].requests.cpu = '200m';
		rows[0].limits.memory = '';
		rows[1].requests.memory = ' 64Mi ';
		rows[1].limits.cpu = '';
		expect(resourceChanges(rows)).toEqual([
			{ container: 'web', requests: { cpu: '200m' }, limits: { memory: null } },
			{ container: 'sidecar', requests: { memory: '64Mi' }, limits: {} }
		]);
	});

	it('finds amounts Kubernetes does not take', () => {
		const rows = rowsOf(web);
		rows[0].requests.cpu = 'much';
		rows[0].limits.memory = '64Mi';
		rows[1].requests.cpu = '0';
		expect(fieldProblems(rows).map((p) => [p.container, p.part, p.measure, p.text])).toEqual([
			['web', 'requests', 'cpu', 'Not an amount of CPU, such as 250m or 1'],
			['web', 'limits', 'memory', 'The limit is below the request'],
			['sidecar', 'requests', 'cpu', 'Not an amount of CPU, such as 250m or 1']
		]);
	});

	it('adds up a pod, with the limit standing in for a missing request', () => {
		const rows = rowsOf(
			deployment([
				{ name: 'a', resources: { requests: { cpu: '100m' }, limits: { memory: '1Gi' } } },
				{ name: 'b', resources: { requests: { cpu: '50m', memory: '64Mi' } } }
			])
		);
		const total = podTotals(rows, true);
		expect(total.cpu).toBeCloseTo(0.15);
		expect(total.memory).toBe(GI + 64 * MI);
		expect(podLimits(rows, true)).toEqual({ cpu: null, memory: null });
		rows[1].limits.memory = '64Mi';
		expect(podLimits(rows, false)).toEqual({ cpu: null, memory: GI + 64 * MI });
	});

	it('counts the largest init container', () => {
		const object = deployment([{ name: 'a', resources: { requests: { cpu: '100m' } } }], [{ name: 'init', resources: { requests: { cpu: '1' } } }]);
		expect(podTotals(rowsOf(object), false, initRequests(object))).toEqual({ cpu: 1, memory: 0 });
	});
});

describe('budget', () => {
	const none = { before: { cpu: null, memory: null }, after: { cpu: null, memory: null } };
	const size = (cpu: number, memory: number) => ({ cpu, memory });

	it('counts what fits once the old pods are gone', () => {
		const capacity: Capacity = {
			pods: 2,
			nodes: [
				node('a', { requested: size(1.5, GI), own: size(0.5, 0.5 * GI), ownPods: 1 }),
				node('b', { requested: size(0.5, 0.5 * GI), own: size(0.5, 0.5 * GI), ownPods: 1 }),
				node('c', { eligible: false, reasons: ['taint'] })
			],
			nodesError: null,
			quotas: []
		};
		const b = budget(capacity, size(0.5, 0.5 * GI), size(1, GI), none);
		expect(b.nodes).toBe(2);
		expect(b.measures[0]).toEqual({ measure: 'cpu', allocatable: 4, others: 1, before: 1, after: 2 });
		// a: 1 core free with its own pod gone, b: 2.
		expect(b.placeable).toBe(3);
		expect(b.warnings).toEqual([]);
	});

	it('warns when pods would stay pending', () => {
		const capacity: Capacity = { pods: 3, nodes: [node('a'), node('b')], nodesError: null, quotas: [] };
		expect(budget(capacity, size(0, 0), size(1.5, GI), none).warnings).toEqual([
			{ level: 'bad', text: 'Only 2 of 3 pods of this size fit on the nodes; the rest stay pending' }
		]);
		expect(budget(capacity, size(0, 0), size(3, GI), none).warnings[0].text).toBe('No node has room for a pod of this size');
		const tainted: Capacity = { ...capacity, nodes: [node('a', { eligible: false })] };
		expect(budget(tainted, size(0, 0), size(1, GI), none).warnings[0].text).toBe('No node takes these pods, whatever their size');
	});

	it('warns when a rollout must wait for room', () => {
		const capacity: Capacity = {
			pods: 1,
			nodes: [node('a', { requested: size(1.5, GI), own: size(1, GI), ownPods: 1 })],
			nodesError: null,
			quotas: []
		};
		const b = budget(capacity, size(1, GI), size(1.2, GI), none);
		expect(b.warnings.map((w) => w.level)).toEqual(['warn']);
		expect(b.warnings[0].text).toMatch(/only fits once an old one is gone/);
	});

	it('warns when a node is nearly full', () => {
		const capacity: Capacity = { pods: 1, nodes: [node('a', { requested: size(1, 0) })], nodesError: null, quotas: [] };
		expect(budget(capacity, size(0.1, 0), size(0.9, 0), none).warnings).toEqual([
			{ level: 'warn', text: 'More than 90% of the CPU on these nodes would be requested' }
		]);
	});

	it('checks one pod per node for a DaemonSet', () => {
		const capacity: Capacity = { pods: 2, nodes: [node('a'), node('b', { requested: size(1.8, 0) })], nodesError: null, quotas: [] };
		const b = budget(capacity, size(0.1, 0), size(0.5, 0), none, true);
		expect(b.placeable).toBe(1);
		expect(b.warnings[0].text).toBe('1 of the nodes have no room for a pod of this size');
	});

	it('checks the namespace quotas', () => {
		const capacity: Capacity = {
			pods: 2,
			nodes: null,
			nodesError: 'forbidden',
			quotas: [
				{ name: 'team', resource: 'requests.memory', hard: 2 * GI, used: GI },
				{ name: 'team', resource: 'limits.cpu', hard: 4, used: 1 }
			]
		};
		const limits = { before: { cpu: 0.5, memory: null }, after: { cpu: null, memory: null } };
		expect(budget(capacity, size(0.1, 0.5 * GI), size(0.1, 1.5 * GI), limits).warnings.map((w) => w.text)).toEqual([
			'The quota team allows 2.0Gi of requests.memory; this needs 3.0Gi',
			'The quota team needs a limit on cpu for every container'
		]);
	});
});

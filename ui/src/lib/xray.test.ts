import { describe, expect, it } from 'vitest';
import type { Health, XrayNode } from './api';
import { fill, kindLabel, meterTone, record } from './pulse';
import { initialExpanded, issuesBelow, kindTag, visibleRows } from './xray';

function node(id: string, kind: string, health: Health = 'ok', children: XrayNode[] = []): XrayNode {
	return {
		id,
		kind,
		name: id.split('/').pop()!,
		namespace: 'shop',
		resource: null,
		health,
		detail: '',
		children
	};
}

// deploy web → rs → two pods, one of them with a missing secret.
const tree = [
	node('web', 'Deployment', 'warning', [
		node('web/rs', 'ReplicaSet', 'warning', [
			node('web/rs/p1', 'Pod', 'ok', [node('web/rs/p1/app', 'Container')]),
			node('web/rs/p2', 'Pod', 'failed', [
				node('web/rs/p2/app', 'Container', 'failed'),
				node('web/rs/p2/token', 'Secret', 'failed')
			])
		])
	]),
	node('api', 'Deployment', 'ok', [
		node('api/rs', 'ReplicaSet', 'ok', [node('api/rs/p', 'Pod', 'ok', [node('api/rs/p/app', 'Container')])])
	])
];

describe('xray tree', () => {
	it('counts problems below a node', () => {
		expect(issuesBelow(tree[0])).toBe(4);
		expect(issuesBelow(tree[1])).toBe(0);
	});

	it('opens workloads down to their pods, and every path to a problem', () => {
		const open = initialExpanded(tree);
		expect([...open].sort()).toEqual(['api', 'api/rs', 'web', 'web/rs', 'web/rs/p2']);
	});

	it('shows only expanded branches', () => {
		const rows = visibleRows(tree, new Set(['web']), {
			text: '',
			issuesOnly: false
		});
		expect(rows.map((r) => [r.node.id, r.depth, r.expanded])).toEqual([
			['web', 0, true],
			['web/rs', 1, false],
			['api', 0, false]
		]);
		expect(rows[1].parent).toBe('web');
	});

	it('keeps filter matches with their ancestors, opened', () => {
		const rows = visibleRows(tree, new Set(), {
			text: 'token',
			issuesOnly: false
		});
		expect(rows.map((r) => r.node.id)).toEqual(['web', 'web/rs', 'web/rs/p2', 'web/rs/p2/token']);
	});

	it('can show only paths to problems', () => {
		const rows = visibleRows(tree, new Set(), { text: '', issuesOnly: true });
		expect(rows.map((r) => r.node.id)).toEqual(['web', 'web/rs', 'web/rs/p2', 'web/rs/p2/app', 'web/rs/p2/token']);
	});

	it('tags rows with short kinds', () => {
		expect(kindTag('Deployment')).toBe('deploy');
		expect(kindTag('PersistentVolumeClaim')).toBe('pvc');
		expect(kindTag('Widget')).toBe('widget');
	});
});

describe('pulses', () => {
	it('names kinds in the plural', () => {
		const pulse = {
			resource: { group: '', version: 'v1', plural: 'x' },
			total: 0,
			ok: 0,
			warning: 0,
			failed: 0,
			error: null
		};
		expect(kindLabel({ ...pulse, kind: 'Pod' })).toBe('Pods');
		expect(kindLabel({ ...pulse, kind: 'PersistentVolumeClaim' })).toBe('Volume claims');
	});

	it('fills meters with measured use, or with requests without metrics', () => {
		expect(fill({ used: 2000, requested: 3000, capacity: 8000 })).toEqual({
			value: 0.25,
			measured: true
		});
		expect(fill({ used: null, requested: 4000, capacity: 8000 })).toEqual({
			value: 0.5,
			measured: false
		});
		expect(fill({ used: 1, requested: 1, capacity: null })).toBeNull();
		expect([meterTone(0.5), meterTone(0.8), meterTone(0.95)]).toEqual(['accent', 'warn', 'bad']);
	});

	it('keeps ten minutes of history', () => {
		const start = 1_000_000;
		let history = record([], 1, start);
		history = record(history, null, start + 1000);
		history = record(history, 2, start + 11 * 60 * 1000);
		expect(history).toEqual([{ at: start + 11 * 60 * 1000, value: 2 }]);
	});
});

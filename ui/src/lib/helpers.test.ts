import { describe, expect, it } from 'vitest';
import type { ContextInfo, ResourceInfo, Row } from './api';
import { matchResources, suggest } from './commands';
import { age, cpu, memory } from './format';
import { cellTone } from './status';
import { filterRows, sortRows } from './table';

function resource(kind: string, plural: string, group = '', shortNames: string[] = []): ResourceInfo {
	return {
		group,
		version: 'v1',
		plural,
		kind,
		singular: kind.toLowerCase(),
		shortNames,
		namespaced: true,
		verbs: ['list', 'watch'],
		subresources: []
	};
}

const resources = [
	resource('Pod', 'pods', '', ['po']),
	resource('Deployment', 'deployments', 'apps', ['deploy']),
	resource('Event', 'events', 'events.k8s.io'),
	resource('Event', 'events', '', ['ev']),
	resource('PodDisruptionBudget', 'poddisruptionbudgets', 'policy', ['pdb'])
];

const contexts: ContextInfo[] = [
	{ name: 'prod', cluster: 'prod', user: null, namespace: 'web', isDefault: true },
	{ name: 'staging', cluster: 'stage', user: null, namespace: null, isDefault: false }
];

describe('command palette', () => {
	it('opens the port-forward list', () => {
		expect(suggest('pf', resources, contexts, [])[0].action).toEqual({ type: 'portforwards' });
		expect(suggest(':portforwards', resources, contexts, [])).toHaveLength(1);
	});

	it('resolves short names exactly', () => {
		expect(matchResources('po', resources)[0].plural).toBe('pods');
		expect(matchResources('deploy', resources)[0].kind).toBe('Deployment');
	});

	it('prefers the core group for ambiguous names', () => {
		expect(matchResources('events', resources)[0].group).toBe('');
	});

	it('accepts group-qualified names', () => {
		expect(matchResources('events.events.k8s.io', resources)[0].group).toBe('events.k8s.io');
	});

	it('lists contexts for ctx', () => {
		const s = suggest(':ctx', resources, contexts, []);
		expect(s.map((x) => x.action)).toEqual([
			{ type: 'context', name: 'prod' },
			{ type: 'context', name: 'staging' }
		]);
		expect(suggest('ctx sta', resources, contexts, [])[0].action).toEqual({ type: 'context', name: 'staging' });
	});

	it('switches namespaces with ns', () => {
		const s = suggest('ns kube', resources, contexts, ['default', 'kube-system']);
		expect(s[0].action).toEqual({ type: 'namespace', name: 'kube-system' });
		expect(suggest('ns ', resources, contexts, ['default'])[0].action).toEqual({ type: 'namespace', name: '' });
	});

	it('opens pulses, optionally in a namespace', () => {
		expect(suggest('pulses', resources, contexts, [])[0].action).toEqual({ type: 'pulses', namespace: undefined });
		expect(suggest(':pu shop', resources, contexts, [])[0].action).toEqual({ type: 'pulses', namespace: 'shop' });
	});

	it('opens xray for the kinds it supports', () => {
		const [first, ...rest] = suggest('xray deploy kube-system', resources, contexts, []);
		expect(first.action).toMatchObject({ type: 'xray', namespace: 'kube-system' });
		expect(first.label).toBe('xray deployments.apps kube-system');
		expect(rest).toHaveLength(0);
		// Kinds XRay cannot start from are not offered.
		expect(suggest('xray', resources, contexts, []).map((s) => s.label)).toEqual(['xray deployments.apps', 'xray pods']);
	});

	it('opens rbac and helm, optionally in a namespace', () => {
		expect(suggest('rbac', resources, contexts, [])[0].action).toEqual({ type: 'rbac', namespace: undefined });
		expect(suggest(':policy shop', resources, contexts, [])[0].action).toEqual({ type: 'rbac', namespace: 'shop' });
		expect(suggest('helm', resources, contexts, [])[0].action).toEqual({ type: 'helm', namespace: undefined });
		expect(suggest('charts shop', resources, contexts, [])[0].action).toEqual({ type: 'helm', namespace: 'shop' });
		// Offered while typed, after resources that match.
		expect(suggest('he', resources, contexts, []).map((s) => s.label)).toContain('helm');
	});

	it('passes a namespace argument to resource commands', () => {
		const s = suggest('po kube-system', resources, contexts, []);
		expect(s[0].action).toMatchObject({ type: 'resource', namespace: 'kube-system' });
	});
});

describe('formatting', () => {
	const now = Date.parse('2026-10-27T12:00:00Z');
	it('formats ages like kubectl', () => {
		expect(age('2026-10-27T11:59:30Z', now)).toBe('30s');
		expect(age('2026-10-27T10:00:00Z', now)).toBe('2h');
		expect(age('2026-10-20T12:00:00Z', now)).toBe('7d');
		expect(age(null, now)).toBe('');
	});

	it('formats usage', () => {
		expect(cpu(105.4)).toBe('105m');
		expect(memory(80 * 1024 * 1024)).toBe('80Mi');
		expect(memory(1.5 * 1024 ** 3)).toBe('1.5Gi');
	});
});

describe('status colors', () => {
	it('colors pod states', () => {
		expect(cellTone('Status', 'Running')).toBe('ok');
		expect(cellTone('Status', 'CrashLoopBackOff')).toBe('bad');
		expect(cellTone('Status', 'Pending')).toBe('warn');
		expect(cellTone('Ready', '1/2')).toBe('warn');
		expect(cellTone('Ready', '2/2')).toBe('ok');
		expect(cellTone('Name', 'Running')).toBe('');
	});
});

function row(name: string, namespace: string, cells: unknown[], created = '2026-10-01T00:00:00Z'): Row {
	return { key: `${namespace}/${name}`, name, namespace, created, labels: { app: name }, cells };
}

describe('table rows', () => {
	const columns = [
		{ name: 'Name', type: 'string', format: 'name', description: '', priority: 0 },
		{ name: 'Restarts', type: 'string', format: '', description: '', priority: 0 },
		{ name: 'Age', type: 'string', format: '', description: '', priority: 0 }
	];
	const rows = [
		row('b', 'x', ['b', '10 (3m ago)', '1d'], '2026-10-02T00:00:00Z'),
		row('a', 'y', ['a', '2', '2d'], '2026-10-01T00:00:00Z'),
		row('c', 'x', ['c', '0', '1h'], '2026-10-03T00:00:00Z')
	];

	it('sorts by namespace and name by default', () => {
		expect(sortRows(rows, columns, { column: -1, descending: false }).map((r) => r.name)).toEqual(['b', 'c', 'a']);
	});

	it('sorts restarts numerically and age by creation time', () => {
		expect(sortRows(rows, columns, { column: 1, descending: true }).map((r) => r.name)).toEqual(['b', 'a', 'c']);
		expect(sortRows(rows, columns, { column: 2, descending: false }).map((r) => r.name)).toEqual(['c', 'b', 'a']);
	});

	it('filters by text and by label', () => {
		expect(filterRows(rows, 'y').map((r) => r.name)).toEqual(['a']);
		expect(filterRows(rows, '-l app=c').map((r) => r.name)).toEqual(['c']);
	});
});

import { describe, expect, it } from 'vitest';
import type { ResourceInfo, Row } from './api';
import { markedActions, withCellActions } from './list-actions';
import type { MenuAction } from './menu';

const deployments: ResourceInfo = {
	group: 'apps',
	version: 'v1',
	plural: 'deployments',
	kind: 'Deployment',
	singular: 'deployment',
	shortNames: ['deploy'],
	namespaced: true,
	verbs: ['list', 'delete'],
	subresources: ['scale']
};
const rows: Row[] = ['api', 'web'].map((name) => ({ key: name, name, namespace: 'shop', created: null, labels: {}, owners: [], cells: [name] }));

function labels(writable: boolean, resource = deployments) {
	return markedActions({
		resource,
		chosen: rows,
		marksProduction: false,
		writable,
		target: (r) => ({ resource: { group: resource.group, version: resource.version, plural: resource.plural }, namespace: r.namespace, name: r.name }),
		markAll: () => {},
		unmarkAll: () => {},
		remove: () => {}
	}).map((a) => a.label);
}

describe('markedActions', () => {
	it('offers changes to the marked rows only where changes are allowed', () => {
		expect(labels(true)).toEqual(['Copy names', 'Copy namespaces', 'Mark all', 'Unmark all', 'Scale 2…', 'Restart 2…', 'Delete 2…']);
		expect(labels(false)).toEqual(['Copy names', 'Copy namespaces', 'Mark all', 'Unmark all']);
	});

	it('copies no namespaces of cluster-wide kinds', () => {
		expect(labels(false, { ...deployments, namespaced: false })).not.toContain('Copy namespaces');
	});
});

describe('withCellActions', () => {
	const run = () => {};
	const actions: MenuAction[] = [
		{ group: 'View', label: 'Describe', keys: ['d'], run },
		{ group: 'Copy', label: 'Copy name', keys: ['c'], run }
	];

	it('puts copying the cell first in the Copy group', () => {
		const menu = withCellActions(actions, 'Status', 'Running', () => []);
		expect(menu.map((a) => a.label)).toEqual(['Describe', 'Copy “Running”', 'Copy the Status column', 'Copy name']);
	});

	it('cuts a long cell short in the menu', () => {
		const [first] = withCellActions([], 'Message', 'x'.repeat(40), () => []);
		expect(first.label).toBe(`Copy “${'x'.repeat(31)}…”`);
		expect(withCellActions([], 'Message', '', () => [])[0].label).toBe('Copy cell');
	});
});

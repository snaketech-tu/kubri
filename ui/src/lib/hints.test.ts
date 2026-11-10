import { describe, expect, it } from 'vitest';
import type { ResourceInfo } from './api';
import { footerHints, viewHints, type HintState } from './hints';

const deployments: ResourceInfo = {
	group: 'apps',
	version: 'v1',
	plural: 'deployments',
	kind: 'Deployment',
	singular: 'deployment',
	shortNames: ['deploy'],
	namespaced: true,
	verbs: ['list', 'update', 'delete'],
	subresources: ['scale']
};

function state(extra: Partial<HintState> = {}): HintState {
	return {
		connecting: null,
		cluster: null,
		contexts: [],
		details: null,
		page: 'resources',
		selected: deployments,
		listRows: 3,
		scope: null,
		readOnly: false,
		k9s: null,
		pluginView: { kind: 'resource', resource: deployments },
		layout: { maximized: false },
		...extra
	} as HintState;
}
const keys = (hints: [string, string][]) => hints.map(([key]) => key);

describe('viewHints', () => {
	it('offers the change keys only where changes are allowed', () => {
		expect(keys(viewHints(state()))).toEqual(expect.arrayContaining(['e', '^d', 's', 'r']));
		expect(keys(viewHints(state({ readOnly: true })))).not.toContain('^d');
	});

	it('offers no row keys for an empty list', () => {
		expect(keys(viewHints(state({ listRows: 0 })))).toEqual([':', '/', '?']);
	});
});

describe('footerHints', () => {
	it('keeps to eight keys, with help last', () => {
		const shown = footerHints(state());
		expect(shown.length).toBeLessThanOrEqual(8);
		expect(shown.at(-1)?.[0]).toBe('?');
		expect(viewHints(state()).length).toBeGreaterThan(8);
	});
});

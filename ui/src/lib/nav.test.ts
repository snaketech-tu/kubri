import { describe, expect, it } from 'vitest';
import type { ResourceInfo } from './api';
import { kindGroups, pluralKind, toolOf } from './nav';

describe('pluralKind', () => {
	it('keeps the kind casing and takes the ending of the plural', () => {
		expect(pluralKind('CustomResourceDefinition', 'customresourcedefinitions')).toBe('CustomResourceDefinitions');
		expect(pluralKind('NetworkPolicy', 'networkpolicies')).toBe('NetworkPolicies');
		expect(pluralKind('Endpoints', 'endpoints')).toBe('Endpoints');
		expect(pluralKind('Ingress', 'ingresses')).toBe('Ingresses');
	});

	it('keeps the kind when the plural is something else', () => {
		expect(pluralKind('Database', 'dbs')).toBe('Database');
	});
});

describe('kindGroups', () => {
	const kind = (kind: string, plural: string, group = '', shortNames: string[] = []): ResourceInfo => ({
		kind,
		plural,
		group,
		version: 'v1',
		singular: kind.toLowerCase(),
		shortNames,
		namespaced: true,
		verbs: ['list', 'watch'],
		subresources: []
	});
	const resources = [
		kind('Deployment', 'deployments', 'apps', ['deploy']),
		kind('Pod', 'pods', '', ['po']),
		kind('Certificate', 'certificates', 'cert-manager.io', ['cert']),
		kind('ConfigMap', 'configmaps', '', ['cm']),
		{ ...kind('Binding', 'bindings'), verbs: ['create'] }
	];

	it('puts the core group first and sorts the kinds', () => {
		expect(kindGroups(resources, '').map((g) => [g.group, g.kinds.map((k) => k.kind)])).toEqual([
			['', ['ConfigMap', 'Pod']],
			['apps', ['Deployment']],
			['cert-manager.io', ['Certificate']]
		]);
	});

	it('finds kinds by name, plural, short name and group', () => {
		expect(kindGroups(resources, 'deploy').flatMap((g) => g.kinds.map((k) => k.kind))).toEqual(['Deployment']);
		expect(kindGroups(resources, 'cm').flatMap((g) => g.kinds.map((k) => k.kind))).toEqual(['ConfigMap']);
		expect(kindGroups(resources, 'cert-manager').flatMap((g) => g.kinds.map((k) => k.kind))).toEqual(['Certificate']);
		expect(kindGroups(resources, 'nothing')).toEqual([]);
	});

	it('leaves out kinds that cannot be listed live', () => {
		expect(kindGroups(resources, 'binding')).toEqual([]);
	});
});

describe('toolOf', () => {
	it('names the tool of a page, with the graph under Topology', () => {
		expect(toolOf('graph')).toBe('xray');
		expect(toolOf('sanitizer')).toBe('sanitizer');
		expect(toolOf('resources')).toBeNull();
	});
});

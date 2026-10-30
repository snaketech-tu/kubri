import { describe, expect, it } from 'vitest';
import { pluralKind } from './nav';

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

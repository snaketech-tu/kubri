import { describe, expect, it } from 'vitest';
import type { SizeAdvice } from './api';
import { byContainer } from './advice';

const advice = (name: string, container: string, resource: 'cpu' | 'memory'): SizeAdvice => ({
	workload: { resource: { group: 'apps', version: 'v1', plural: 'deployments' }, namespace: 'shop', name },
	kind: 'Deployment',
	container,
	resource,
	request: null,
	limit: null,
	requested: null,
	used: 0,
	suggestedRequest: null,
	suggestedLimit: null,
	reason: 'noRequest',
	freed: 0
});

describe('byContainer', () => {
	it('puts the CPU and memory of one container together, keeping the order of the first', () => {
		const rows = byContainer([advice('db', 'db', 'cpu'), advice('web', 'web', 'cpu'), advice('db', 'db', 'memory')]);
		expect(rows.map((r) => [r.advice.workload.name, r.advice.resource, r.first])).toEqual([
			['db', 'cpu', true],
			['db', 'memory', false],
			['web', 'cpu', true]
		]);
	});
});

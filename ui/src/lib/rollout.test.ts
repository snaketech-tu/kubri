import { describe, expect, it } from 'vitest';
import type { KubeObject } from './api';
import { imageChanges, revisionsOf } from './rollout';

const rs = (name: string, revision: string | undefined, image: string): KubeObject =>
	({
		metadata: { name, annotations: revision ? { 'deployment.kubernetes.io/revision': revision } : {}, creationTimestamp: '2026-01-01T00:00:00Z' },
		spec: { template: { spec: { containers: [{ image }] } } },
		status: { replicas: 1 }
	}) as KubeObject;

describe('revisionsOf', () => {
	it('lists revisions newest first and marks the current one', () => {
		const deployment = { metadata: { name: 'web', annotations: { 'deployment.kubernetes.io/revision': '3' } } } as KubeObject;
		const list = revisionsOf(deployment, [rs('a', '1', 'web:1'), rs('c', '3', 'web:3'), rs('b', '2', 'web:2'), rs('x', undefined, 'web:0')]);
		expect(list.map((r) => r.revision)).toEqual([3, 2, 1]);
		expect(list.map((r) => r.current)).toEqual([true, false, false]);
	});
});

describe('imageChanges', () => {
	it('names only what differs', () => {
		expect(imageChanges(['web:1', 'side:1'], ['web:2', 'side:1'])).toEqual(['web:1 → web:2']);
		expect(imageChanges(['web:1'], ['web:1'])).toEqual([]);
	});
});

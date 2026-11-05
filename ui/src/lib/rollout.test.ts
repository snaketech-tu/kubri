import { describe, expect, it } from 'vitest';
import type { KubeObject } from './api';
import { controllerRevisionsOf, imageChanges, revisionsOf } from './rollout';

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

const cr = (name: string, revision: number, image: string): KubeObject =>
	({
		metadata: { name, creationTimestamp: '2026-01-01T00:00:00Z' },
		revision,
		data: { spec: { template: { spec: { containers: [{ image }] } } } }
	}) as unknown as KubeObject;

describe('controllerRevisionsOf', () => {
	it('marks the revision a StatefulSet runs', () => {
		const sts = { metadata: { name: 'db' }, status: { updateRevision: 'db-b' } } as unknown as KubeObject;
		const list = controllerRevisionsOf(sts, [cr('db-a', 1, 'db:1'), cr('db-c', 3, 'db:3'), cr('db-b', 2, 'db:2')]);
		expect(list.map((r) => [r.revision, r.current, r.images[0]])).toEqual([
			[3, false, 'db:3'],
			[2, true, 'db:2'],
			[1, false, 'db:1']
		]);
		expect(list[0].replicas).toBeNull();
	});

	it('takes the newest revision of a DaemonSet as current', () => {
		const ds = { metadata: { name: 'agent' }, status: {} } as unknown as KubeObject;
		expect(controllerRevisionsOf(ds, [cr('agent-a', 1, 'a:1'), cr('agent-b', 2, 'a:2')]).map((r) => r.current)).toEqual([true, false]);
	});
});

describe('imageChanges', () => {
	it('names only what differs', () => {
		expect(imageChanges(['web:1', 'side:1'], ['web:2', 'side:1'])).toEqual(['web:1 → web:2']);
		expect(imageChanges(['web:1'], ['web:1'])).toEqual([]);
	});
});

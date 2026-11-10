import { describe, expect, it } from 'vitest';
import type { ResourceInfo, SearchHit } from './api';
import { forwardKinds, lastEnd, targetKey, targetOptions } from './forwards';

const now = Date.parse('2026-11-05T10:00:00Z');

describe('how a forward connection ended', () => {
	it('says who closed it and when', () => {
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:58:00Z', by: 'pod' } }, now)).toBe('Last connection closed by the pod, 2m ago');
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:59:30Z', by: 'client' } }, now)).toBe('Last connection closed by the client, 30s ago');
	});

	it('leaves errors to the error line, and says nothing before the first end', () => {
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:59:30Z', by: 'error' } }, now)).toBeNull();
		expect(lastEnd({ lastEnd: null }, now)).toBeNull();
	});
});

describe('targets of a new forward', () => {
	const info = (group: string, plural: string, kind: string) => ({ group, version: 'v1', plural, kind }) as unknown as ResourceInfo;
	const deployments = info('apps', 'deployments', 'Deployment');
	const services = info('', 'services', 'Service');
	const pods = info('', 'pods', 'Pod');
	const hit = (r: ResourceInfo, name: string): SearchHit => ({
		object: { resource: { group: r.group, version: r.version, plural: r.plural }, namespace: 'shop', name },
		kind: r.kind,
		created: null
	});

	it('offers workloads first, then services and pods, and leaves out other kinds', () => {
		const resources = [pods, info('apps', 'replicasets', 'ReplicaSet'), services, info('', 'configmaps', 'ConfigMap'), deployments];
		expect(forwardKinds(resources).map((r) => r.kind)).toEqual(['Deployment', 'Service', 'Pod']);
	});

	it('groups the found objects by kind, sorted by name', () => {
		const options = targetOptions([hit(pods, 'web-2'), hit(deployments, 'web'), hit(pods, 'api-1'), hit(services, 'web')], [deployments, services, pods]);
		expect(options).toEqual([
			{ value: 'apps/deployments/web', label: 'web', group: 'Deployment' },
			{ value: '/services/web', label: 'web', group: 'Service' },
			{ value: '/pods/api-1', label: 'api-1', group: 'Pod' },
			{ value: '/pods/web-2', label: 'web-2', group: 'Pod' }
		]);
		expect(targetKey({ resource: deployments, name: 'web' })).toBe('apps/deployments/web');
	});
});

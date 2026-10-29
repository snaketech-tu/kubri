import { describe, expect, it } from 'vitest';
import type { HelmRelease, ResourceInfo } from './api';
import {
	filterReleases,
	hasValues,
	manifestChanges,
	manifestObjects,
	manifestResource,
	previousRevision,
	statusHealth,
	valuesDiffer
} from './helm';

function release(name: string, revision: number, status = 'deployed'): HelmRelease {
	return {
		name,
		namespace: 'shop',
		revision,
		status,
		chart: 'nginx',
		chartVersion: '1.2.3',
		appVersion: '1.27',
		updated: null,
		description: ''
	};
}

describe('helm', () => {
	it('maps statuses to health, leaving history neutral', () => {
		expect(statusHealth('deployed')).toBe('ok');
		expect(statusHealth('failed')).toBe('failed');
		expect(statusHealth('pending-upgrade')).toBe('warning');
		expect(statusHealth('superseded')).toBeNull();
	});

	it('filters releases by name, chart and status', () => {
		const releases = [release('web', 3), release('db', 1, 'failed')];
		expect(filterReleases(releases, 'fail').map((r) => r.name)).toEqual(['db']);
		expect(filterReleases(releases, 'NGINX')).toHaveLength(2);
		expect(filterReleases(releases, '1.2.3')).toHaveLength(2);
	});

	it('tells given values from none', () => {
		expect(hasValues({})).toBe(false);
		expect(hasValues(null)).toBe(false);
		expect(hasValues({ replicas: 2 })).toBe(true);
	});

	it('rolls back to the newest older revision', () => {
		const history = [release('web', 5), release('web', 4), release('web', 2)];
		expect(previousRevision(history, 5)).toBe(4);
		expect(previousRevision(history, 4)).toBe(2);
		expect(previousRevision(history, 2)).toBeNull();
	});

	it('lists the objects of a rendered manifest', () => {
		const manifest = [
			'---',
			'# Source: web/templates/service.yaml',
			'apiVersion: v1',
			'kind: Service',
			'metadata:',
			'  name: web',
			'---',
			'# Source: web/templates/deployment.yaml',
			'apiVersion: apps/v1',
			'kind: Deployment',
			'metadata:',
			'  name: web',
			'  namespace: other',
			'---',
			'# Source: web/templates/empty.yaml',
			''
		].join('\n');
		const objects = manifestObjects(manifest);
		expect(objects).toEqual([
			{ apiVersion: 'v1', kind: 'Service', name: 'web', namespace: null },
			{ apiVersion: 'apps/v1', kind: 'Deployment', name: 'web', namespace: 'other' }
		]);
		const resources = [{ group: 'apps', kind: 'Deployment', plural: 'deployments' }] as ResourceInfo[];
		expect(manifestResource(resources, objects[1])?.plural).toBe('deployments');
		expect(manifestResource(resources, objects[0])).toBeUndefined();
	});
});

describe('manifest changes', () => {
	const deploy = (replicas: number) => `---
# Source: shop/templates/deploy.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: shop
spec:
  replicas: ${replicas}
`;
	const svc = `---
apiVersion: v1
kind: Service
metadata:
  name: shop
spec:
  ports: [{ port: 80 }]
`;
	const map = (namespace?: string) => `---
apiVersion: v1
kind: ConfigMap
metadata:
  name: shop
${namespace ? `  namespace: ${namespace}\n` : ''}data: { a: "1" }
`;

	it('pairs objects by kind, namespace and name, changes first', () => {
		const changes = manifestChanges(svc + deploy(1) + map(), deploy(3) + svc + '---\nkind: Secret\napiVersion: v1\nmetadata: { name: shop }\n', 'apps');
		expect(changes.map((c) => [c.kind, c.change])).toEqual([
			['Deployment', 'changed'],
			['Secret', 'added'],
			['ConfigMap', 'removed'],
			['Service', 'unchanged']
		]);
		expect((changes[0].before as any).spec.replicas).toBe(1);
		expect((changes[0].after as any).spec.replicas).toBe(3);
	});

	it('treats the release namespace and an explicit one alike', () => {
		expect(manifestChanges(map(), map('apps'), 'apps').map((c) => c.change)).toEqual(['changed']);
		expect(manifestChanges(map(), map('other'), 'apps').map((c) => c.change)).toEqual(['added', 'removed']);
	});

	it('compares values without caring about key order or empty values', () => {
		expect(valuesDiffer({ a: 1, b: { c: 2, d: 3 } }, { b: { d: 3, c: 2 }, a: 1 })).toBe(false);
		expect(valuesDiffer(null, {})).toBe(false);
		expect(valuesDiffer({ a: 1 }, { a: 2 })).toBe(true);
	});
});

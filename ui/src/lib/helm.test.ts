import { describe, expect, it } from 'vitest';
import type { HelmRelease, ResourceInfo } from './api';
import { filterReleases, hasValues, manifestObjects, manifestResource, previousRevision, statusHealth } from './helm';

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

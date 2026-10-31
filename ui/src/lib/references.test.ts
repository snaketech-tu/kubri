import { describe, expect, it } from 'vitest';
import type { Reference } from './api';
import { counted, relatedOf, summarizeUsers } from './references';

const ref = (kind: string, name: string, how?: string): Reference => ({
	resource: { group: '', version: 'v1', plural: `${kind.toLowerCase()}s` },
	kind,
	namespace: 'shop',
	name,
	how
});

describe('summarizeUsers', () => {
	it('counts users by kind and how they use it', () => {
		expect(summarizeUsers([ref('Deployment', 'a'), ref('Deployment', 'b'), ref('Deployment', 'c'), ref('Ingress', 'shop', 'TLS')])).toBe(
			'3 Deployments and 1 Ingress for TLS'
		);
		expect(summarizeUsers([ref('Pod', 'x'), ref('CronJob', 'y'), ref('Ingress', 'a', 'backend'), ref('Ingress', 'b', 'backend')])).toBe(
			'1 Pod, 1 CronJob and 2 Ingresses for backend'
		);
		expect(summarizeUsers([])).toBe('');
	});

	it('writes plurals', () => {
		expect(counted(1, 'Ingress')).toBe('1 Ingress');
		expect(counted(2, 'Ingress')).toBe('2 Ingresses');
		expect(counted(2, 'HTTPRoute')).toBe('2 HTTPRoutes');
	});
});

describe('relatedOf', () => {
	const pod = {
		kind: 'Pod',
		spec: {
			serviceAccountName: 'builder',
			volumes: [{ configMap: { name: 'cfg' } }, { secret: { secretName: 'tls' } }, { persistentVolumeClaim: { claimName: 'data' } }],
			containers: [{ envFrom: [{ configMapRef: { name: 'cfg' } }], env: [{ valueFrom: { secretKeyRef: { name: 'db' } } }] }]
		}
	};
	it('lists what a pod names, each once', () => {
		const names = relatedOf(pod).map((r) => `${r.kind}/${r.name}`);
		expect(names).toEqual(['ConfigMap/cfg', 'Secret/tls', 'PersistentVolumeClaim/data', 'Secret/db', 'ServiceAccount/builder']);
	});
	it('reads the pod template of a workload', () => {
		expect(relatedOf({ kind: 'Deployment', spec: { template: { spec: pod.spec } } })).toHaveLength(5);
	});
	it('ignores the default service account and objects without a spec', () => {
		expect(relatedOf({ kind: 'Pod', spec: { serviceAccountName: 'default' } })).toEqual([]);
		expect(relatedOf({ kind: 'ConfigMap' })).toEqual([]);
	});
});

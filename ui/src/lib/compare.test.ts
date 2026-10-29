import { describe, expect, it } from 'vitest';
import { comparable, imageChanges } from './compare';

const deploy = (image: string, extra: Record<string, unknown> = {}) => ({
	apiVersion: 'apps/v1',
	kind: 'Deployment',
	metadata: {
		name: 'web',
		namespace: 'staging',
		uid: 'a',
		resourceVersion: '12',
		generation: 3,
		creationTimestamp: '2026-10-01T00:00:00Z',
		annotations: { 'deployment.kubernetes.io/revision': '4' },
		...extra
	},
	spec: {
		replicas: 2,
		template: {
			metadata: { annotations: { 'kubectl.kubernetes.io/restartedAt': 'x' } },
			spec: { initContainers: [{ name: 'migrate', image }], containers: [{ name: 'web', image }] }
		}
	},
	status: { readyReplicas: 2 }
});

describe('comparable', () => {
	it('leaves out what each copy has of its own', () => {
		expect(comparable(deploy('nginx:1.27'))).toEqual({
			apiVersion: 'apps/v1',
			kind: 'Deployment',
			metadata: { name: 'web' },
			spec: { replicas: 2, template: { metadata: {}, spec: { initContainers: [{ name: 'migrate', image: 'nginx:1.27' }], containers: [{ name: 'web', image: 'nginx:1.27' }] } } }
		});
		expect(comparable(deploy('nginx:1.27'), true).status).toEqual({ readyReplicas: 2 });
	});

	it('keeps annotations that are the user’s', () => {
		const c = comparable(deploy('a', { annotations: { owner: 'team-a', 'deployment.kubernetes.io/revision': '1' } }));
		expect((c.metadata as { annotations: unknown }).annotations).toEqual({ owner: 'team-a' });
	});
});

describe('imageChanges', () => {
	it('lists containers whose image differs', () => {
		expect(imageChanges(deploy('nginx:1.27'), deploy('nginx:1.28'))).toEqual([
			{ container: 'migrate', left: 'nginx:1.27', right: 'nginx:1.28' },
			{ container: 'web', left: 'nginx:1.27', right: 'nginx:1.28' }
		]);
		expect(imageChanges(deploy('a'), deploy('a'))).toEqual([]);
		const pod = { kind: 'Pod', spec: { containers: [{ name: 'app', image: 'b' }] } };
		expect(imageChanges(pod, null)).toEqual([{ container: 'app', left: 'b', right: null }]);
	});
});

import { describe, expect, it } from 'vitest';
import type { KubeObject, ResourceInfo } from './api';
import { writeQueue } from './api/queue';
import {
	AUTO_PORT_FORWARDS_KEY,
	PORT_FORWARDS_KEY,
	annotatedForwards,
	annotatedPorts,
	containerImages,
	containerNames,
	forwardablePorts,
	hasLogs,
	registryOf,
	logMatcher,
	logRanges,
	secretValue,
	sourceLabels,
	stripAnsi,
	suggestedLocalPort
} from './pods';

function resource(kind: string, plural: string, group = '', namespaced = true): ResourceInfo {
	return { group, version: 'v1', plural, kind, singular: '', shortNames: [], namespaced, verbs: [], subresources: [] };
}

const pod: KubeObject = {
	kind: 'Pod',
	metadata: { name: 'web-1' },
	spec: {
		initContainers: [{ name: 'migrate' }],
		containers: [
			{ name: 'app', ports: [{ name: 'http', containerPort: 8080 }, { containerPort: 53, protocol: 'UDP' }] },
			{ name: 'proxy', ports: [{ containerPort: 15001 }] }
		]
	}
};

describe('logs', () => {
	it('knows which objects have logs', () => {
		expect(hasLogs(resource('Pod', 'pods'))).toBe(true);
		expect(hasLogs(resource('Deployment', 'deployments', 'apps'))).toBe(true);
		expect(hasLogs(resource('ConfigMap', 'configmaps'))).toBe(false);
		expect(hasLogs(resource('Node', 'nodes', '', false))).toBe(false);
		const rollout = resource('Rollout', 'rollouts', 'argoproj.io');
		expect(hasLogs(rollout, { metadata: { name: 'r' }, spec: { selector: { matchLabels: {} } } })).toBe(true);
		expect(hasLogs(rollout, { metadata: { name: 'r' }, spec: {} })).toBe(false);
	});

	it('lists containers of pods and pod templates', () => {
		expect(containerNames(pod)).toEqual(['app', 'proxy', 'migrate']);
		const deployment: KubeObject = {
			kind: 'Deployment',
			metadata: { name: 'web' },
			spec: { template: { spec: { containers: [{ name: 'app' }] } } }
		};
		expect(containerNames(deployment)).toEqual(['app']);
		expect(containerNames(null)).toEqual([]);
	});

	it('filters by substring or regex', () => {
		expect(logMatcher('')('anything')).toBe(true);
		expect(logMatcher('error')('Some ERROR here')).toBe(true);
		expect(logMatcher('error')('all good')).toBe(false);
		expect(logMatcher('/^GET \\/api/')('GET /api/pods')).toBe(true);
		expect(logMatcher('/^GET \\/api/')('POST /api/pods')).toBe(false);
		expect(logMatcher('/[unclosed/')('path /[unclosed/ here')).toBe(true);
	});

	it('strips color codes', () => {
		expect(stripAnsi('\x1b[32mINFO\x1b[0m ready \x1b[1;31m!\x1b[m')).toBe('INFO ready !');
	});

	it('labels sources as briefly as possible', () => {
		expect(sourceLabels([{ pod: 'a', container: 'app' }, { pod: 'a', container: 'proxy' }])).toEqual(['app', 'proxy']);
		expect(sourceLabels([{ pod: 'a', container: 'app' }, { pod: 'b', container: 'app' }])).toEqual(['a', 'b']);
		expect(
			sourceLabels([
				{ pod: 'a', container: 'app' },
				{ pod: 'a', container: 'proxy' },
				{ pod: 'b', container: 'app' }
			])
		).toEqual(['a/app', 'a/proxy', 'b/app']);
	});
});

describe('port-forwards', () => {
	it('offers TCP container ports and service ports', () => {
		expect(forwardablePorts(pod)).toEqual([
			{ port: 8080, name: 'http', detail: 'app' },
			{ port: 15001, name: null, detail: 'proxy' }
		]);
		const service: KubeObject = {
			kind: 'Service',
			metadata: { name: 'web' },
			spec: { ports: [{ name: 'web', port: 80, targetPort: 'http' }, { port: 53, protocol: 'UDP' }] }
		};
		expect(forwardablePorts(service)).toEqual([{ port: 80, name: 'web', detail: 'targets http' }]);
	});

	it('suggests unprivileged local ports', () => {
		expect(suggestedLocalPort(8080)).toBe(8080);
		expect(suggestedLocalPort(80)).toBe(null);
	});
});

describe('k9s port-forward annotations', () => {
	it('reads the forms k9s takes', () => {
		expect(annotatedPorts('8080, 9090:http, nginx::8443:443, bad entry')).toEqual([
			{ container: null, local: null, port: 8080 },
			{ container: null, local: 9090, port: 'http' },
			{ container: 'nginx', local: 8443, port: 443 }
		]);
		expect(annotatedPorts(undefined)).toEqual([]);
	});

	it('looks port names up in the containers, of pods and pod templates', () => {
		const annotated = (annotations: Record<string, string>) => ({ ...pod, metadata: { name: 'web-1', annotations } });
		expect(annotatedForwards(annotated({ [PORT_FORWARDS_KEY]: '9000:http,proxy::15001' }), PORT_FORWARDS_KEY)).toEqual([
			{ remote: 8080, local: 9000 },
			{ remote: 15001, local: null }
		]);
		// A name the container does not have is left out.
		expect(annotatedForwards(annotated({ [PORT_FORWARDS_KEY]: 'proxy::http' }), PORT_FORWARDS_KEY)).toEqual([]);
		const deployment: KubeObject = {
			kind: 'Deployment',
			metadata: { name: 'web' },
			spec: {
				template: {
					metadata: { annotations: { [AUTO_PORT_FORWARDS_KEY]: 'http' } },
					spec: { containers: [{ name: 'app', ports: [{ name: 'http', containerPort: 80 }] }] }
				}
			}
		};
		expect(annotatedForwards(deployment, AUTO_PORT_FORWARDS_KEY)).toEqual([{ remote: 80, local: null }]);
		expect(annotatedForwards(deployment, PORT_FORWARDS_KEY)).toEqual([]);
	});
});

describe('container images', () => {
	it('lists containers with their images, init containers marked', () => {
		const p: KubeObject = {
			kind: 'Pod',
			metadata: { name: 'p' },
			spec: { initContainers: [{ name: 'init', image: 'busybox' }], containers: [{ name: 'app', image: 'nginx:1.27' }] }
		};
		expect(containerImages(p)).toEqual([
			{ name: 'app', image: 'nginx:1.27', init: false },
			{ name: 'init', image: 'busybox', init: true }
		]);
	});
});

describe('log ranges', () => {
	it('start with the k9s logger tail, or its sinceSeconds', () => {
		const tail = logRanges(100, -1);
		expect(tail.start).toBe('tail');
		expect([...tail.ranges.keys()]).toEqual(['tail', '500', '2000', '1m', '5m', '15m', '30m', '1h', 'all']);
		expect(tail.ranges.get('tail')).toEqual({ label: 'Last 100 lines', options: { tailLines: 100 } });
		expect(logRanges(500, 300).start).toBe('5m');
		expect([...logRanges(500, 300).ranges.keys()]).not.toContain('500');
		const custom = logRanges(100, 7200);
		expect(custom.start).toBe('since');
		expect(custom.ranges.get('since')?.label).toBe('Last 2 hours');
	});
});

describe('secret values', () => {
	it('decodes text and tells binary apart', () => {
		expect(secretValue(btoa('s3cret'))).toEqual({ bytes: 6, text: 's3cret' });
		expect(secretValue(btoa(String.fromCharCode(0xc3, 0xa9)))).toEqual({ bytes: 2, text: 'é' });
		expect(secretValue(btoa(String.fromCharCode(0xff, 0xfe, 0x00)))).toEqual({ bytes: 3, text: null });
		expect(secretValue('not base64!')).toEqual({ bytes: 0, text: null });
	});
});

describe('terminal input', () => {
	it('keeps keystrokes in order and batches them while a write is pending', async () => {
		const sent: string[] = [];
		let release: () => void = () => {};
		const queue = writeQueue(
			(data) => {
				sent.push(data);
				return new Promise<void>((resolve) => (release = resolve));
			},
			() => {}
		);
		queue.write('l');
		expect(sent).toEqual([]);
		queue.start();
		queue.write('s');
		queue.write(' -la\r');
		expect(sent).toEqual(['l']);
		release();
		await new Promise((r) => setTimeout(r));
		expect(sent).toEqual(['l', 's -la\r']);
	});
});

describe('registryOf', () => {
	it('reads the registry host, or Docker Hub', () => {
		expect(registryOf('busybox:1.36')).toBe('docker.io');
		expect(registryOf('bitnami/redis')).toBe('docker.io');
		expect(registryOf('ghcr.io/snaketech-tu/kubri:0.4')).toBe('ghcr.io');
		expect(registryOf('localhost:5000/app')).toBe('localhost:5000');
		expect(registryOf('registry.local/team/app@sha256:abc')).toBe('registry.local');
	});
});

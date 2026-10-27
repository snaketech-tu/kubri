import { describe, expect, it } from 'vitest';
import type { KubeObject, ResourceInfo } from './api';
import { writeQueue } from './api/queue';
import {
	containerNames,
	forwardablePorts,
	hasLogs,
	logMatcher,
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

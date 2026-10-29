import { describe, expect, it } from 'vitest';
import type { ObjectRef } from './api';
import { helmCommand, kubectlCommand, shellQuote } from './kubectl';

const deploy: ObjectRef = { resource: { group: 'apps', version: 'v1', plural: 'deployments' }, namespace: 'shop', name: 'web' };
const pod: ObjectRef = { resource: { group: '', version: 'v1', plural: 'pods' }, namespace: 'shop', name: 'web-1' };
const node: ObjectRef = { resource: { group: '', version: 'v1', plural: 'nodes' }, namespace: null, name: 'worker-1' };

describe('kubectlCommand', () => {
	it('scopes commands to the context and namespace', () => {
		expect(kubectlCommand('prod', deploy, { op: 'scale', replicas: 3 })).toBe(
			'kubectl --context prod -n shop scale deployments.apps/web --replicas=3'
		);
		expect(kubectlCommand(null, pod, { op: 'describe' })).toBe('kubectl -n shop describe pods/web-1');
		expect(kubectlCommand('prod', node, { op: 'cordon' })).toBe('kubectl --context prod cordon worker-1');
	});

	it('writes placeholders for values not chosen yet', () => {
		expect(kubectlCommand('c', deploy, { op: 'scale' })).toContain('--replicas=REPLICAS');
		expect(kubectlCommand('c', deploy, { op: 'image' })).toContain('CONTAINER=IMAGE');
	});

	it('covers the changing actions', () => {
		expect(kubectlCommand('c', deploy, { op: 'restart' })).toBe('kubectl --context c -n shop rollout restart deployments.apps/web');
		expect(kubectlCommand('c', deploy, { op: 'image', images: [{ name: 'web', image: 'nginx:1.27', init: false }] })).toBe(
			'kubectl --context c -n shop set image deployments.apps/web web=nginx:1.27'
		);
		expect(kubectlCommand('c', pod, { op: 'labels', labels: { tier: 'front', old: null }, annotations: { note: 'a b' } })).toBe(
			"kubectl --context c -n shop label --overwrite pods/web-1 tier=front old-\nkubectl --context c -n shop annotate --overwrite pods/web-1 'note=a b'"
		);
		expect(kubectlCommand('c', deploy, { op: 'suspend', suspend: true })).toBe(
			`kubectl --context c -n shop patch deployments.apps/web --type=merge -p '{"spec":{"suspend":true}}'`
		);
		expect(kubectlCommand('c', node, { op: 'drain', options: { ignoreDaemonsets: true, force: true, gracePeriodSeconds: 30 } })).toBe(
			'kubectl --context c drain worker-1 --ignore-daemonsets --force --grace-period=30'
		);
	});

	it('debugs like kubectl debug', () => {
		expect(kubectlCommand('k3s', pod, { op: 'debug', container: 'app', image: 'busybox:1.36' })).toBe(
			'kubectl --context k3s -n shop debug -it web-1 --image=busybox:1.36 --target=app'
		);
		expect(kubectlCommand(null, pod, { op: 'debug', copy: true, container: 'app' })).toBe(
			'kubectl -n shop debug web-1 --copy-to=web-1-debug --container=app -it -- sh'
		);
		const node: ObjectRef = { resource: { group: '', version: 'v1', plural: 'nodes' }, name: 'n1' };
		expect(kubectlCommand(null, node, { op: 'nodeShell' })).toBe(
			'kubectl debug node/n1 -it --profile=sysadmin --image=busybox -- chroot /host sh'
		);
	});

	it('deletes several objects per namespace', () => {
		const other = { ...pod, namespace: 'web', name: 'x' };
		expect(kubectlCommand('c', pod, { op: 'delete', targets: [pod, other, { ...pod, name: 'web-2' }], options: { force: true, gracePeriodSeconds: 0, propagation: 'foreground' } })).toBe(
			'kubectl --context c -n shop delete pods/web-1 pods/web-2 --force --grace-period=0 --cascade=foreground\n' +
				'kubectl --context c -n web delete pods/x --force --grace-period=0 --cascade=foreground'
		);
	});

	it('connects to pods', () => {
		expect(kubectlCommand('c', pod, { op: 'shell', container: 'app' })).toBe("kubectl --context c -n shop exec -it pods/web-1 -c app -- sh -c 'bash || sh'");
		expect(kubectlCommand('c', pod, { op: 'portForward', local: 8080, remote: 80 })).toBe('kubectl --context c -n shop port-forward pods/web-1 8080:80');
		expect(kubectlCommand('c', pod, { op: 'portForward', remote: 80, address: '0.0.0.0' })).toBe(
			'kubectl --context c -n shop port-forward pods/web-1 80 --address 0.0.0.0'
		);
		expect(kubectlCommand('c', deploy, { op: 'logs', workload: true })).toBe('kubectl --context c -n shop logs deployments.apps/web --all-pods --prefix -f');
	});
});

describe('shellQuote', () => {
	it('quotes only what needs it', () => {
		expect(shellQuote('web-1')).toBe('web-1');
		expect(shellQuote('a b')).toBe("'a b'");
		expect(shellQuote("it's")).toBe("'it'\\''s'");
		expect(shellQuote('')).toBe("''");
	});
});

describe('helmCommand', () => {
	it('names the release, namespace and context', () => {
		expect(helmCommand('prod', { name: 'shop', namespace: 'apps' }, { op: 'rollback', revision: 3 })).toBe(
			'helm rollback shop 3 -n apps --kube-context prod'
		);
	});
});

import { describe, expect, it } from 'vitest';
import { attentionTitle, autoscalerCause, containerCause, missingConfig, pendingCause } from './causes';

function pod(status: Record<string, unknown>, spec: Record<string, unknown> = {}) {
	return {
		metadata: { name: 'p', namespace: 'shop' },
		spec: { containers: [{ name: 'app', image: 'busybox:does-not-exist' }], ...spec },
		status: { phase: 'Running', ...status }
	};
}

describe('containerCause', () => {
	it('names the image a container cannot pull', () => {
		const cause = containerCause(pod({ containerStatuses: [{ name: 'app', state: { waiting: { reason: 'ImagePullBackOff', message: 'Back-off' } } }] }), 'app');
		expect(cause?.title).toBe('app never started: the image busybox:does-not-exist cannot be pulled');
		expect(cause?.fix).toBe('image');
	});

	it('names the missing Secret or key', () => {
		const status = (message: string) => pod({ containerStatuses: [{ name: 'app', state: { waiting: { reason: 'CreateContainerConfigError', message } } }] });
		const secret = containerCause(status('secret "api-token" not found'), 'app');
		expect(secret?.title).toBe('app never started: the Secret api-token it needs does not exist');
		expect(secret?.missing).toEqual({ kind: 'Secret', name: 'api-token', key: null, keys: [] });
		const key = containerCause(status("couldn't find key token in ConfigMap shop/settings"), 'app');
		expect(key?.title).toBe('app never started: the key token in the ConfigMap settings it needs does not exist');
		expect(containerCause(status('something else'), 'app')?.fix).toBe('events');
		// The keys come from the container's environment when the message names none.
		const env = pod(
			{ containerStatuses: [{ name: 'app', state: { waiting: { reason: 'CreateContainerConfigError', message: 'secret "api-token" not found' } } }] },
			{ containers: [{ name: 'app', env: [{ name: 'TOKEN', valueFrom: { secretKeyRef: { name: 'api-token', key: 'token' } } }] }] }
		);
		expect(containerCause(env, 'app')?.missing?.keys).toEqual(['token']);
	});

	it('tells out of memory from other crashes', () => {
		const oom = pod(
			{ containerStatuses: [{ name: 'app', restartCount: 3, state: { waiting: { reason: 'CrashLoopBackOff' } }, lastState: { terminated: { reason: 'OOMKilled', exitCode: 137 } } }] },
			{ containers: [{ name: 'app', resources: { limits: { memory: '32Mi' } } }] }
		);
		expect(containerCause(oom, 'app')).toMatchObject({ title: 'app ran out of memory and was stopped', fix: 'memory' });
		expect(containerCause(oom, 'app')?.detail).toContain('32Mi');
		const crash = pod({ containerStatuses: [{ name: 'app', restartCount: 1, state: { waiting: { reason: 'CrashLoopBackOff' } }, lastState: { terminated: { reason: 'Error', exitCode: 1 } } }] });
		expect(containerCause(crash, 'app')).toMatchObject({ title: 'app keeps crashing', fix: 'previousLogs' });
		expect(containerCause(crash, 'app')?.detail).toContain('exit code 1 and restarted 1 time');
	});

	it('says nothing for running or starting containers', () => {
		expect(containerCause(pod({ containerStatuses: [{ name: 'app', state: { running: {} } }] }), 'app')).toBeNull();
		expect(containerCause(pod({ containerStatuses: [{ name: 'app', state: { waiting: { reason: 'ContainerCreating' } } }] }), 'app')).toBeNull();
		expect(containerCause(pod({}), 'app')).toBeNull();
	});
});

describe('pendingCause', () => {
	const pending = (message: string) =>
		pod(
			{ phase: 'Pending', conditions: [{ type: 'PodScheduled', status: 'False', reason: 'Unschedulable', message }] },
			{ containers: [{ name: 'batch', resources: { requests: { cpu: '64', memory: '1Gi' } } }] }
		);

	it('says how much the pod asks for', () => {
		const cause = pendingCause(pending('0/1 nodes are available: 1 Insufficient cpu. preemption: 0/1 nodes are available.'));
		expect(cause).toMatchObject({ title: 'No node has room: the pod asks for 64 CPU, more than any node has free', fix: 'requests', resource: 'cpu' });
	});

	it('knows taints, selectors and volumes', () => {
		expect(pendingCause(pending('0/3 nodes are available: 3 node(s) had untolerated taint {gpu: true}.'))?.title).toContain('taints');
		expect(pendingCause(pending("0/3 nodes are available: 3 node(s) didn't match Pod's node affinity/selector."))?.title).toContain('selector');
		expect(pendingCause(pending('0/1 nodes are available: pod has unbound immediate PersistentVolumeClaims.'))?.fix).toBe('events');
	});

	it('says nothing for scheduled pods', () => {
		expect(pendingCause(pod({ phase: 'Running' }))).toBeNull();
		expect(pendingCause(pod({ phase: 'Pending', conditions: [{ type: 'PodScheduled', status: 'True' }] }))).toBeNull();
	});
});

describe('autoscalerCause', () => {
	const hpa = (message: string, status = 'False') => ({
		metadata: { name: 'web' },
		status: { conditions: [{ type: 'ScalingActive', status, reason: 'FailedGetResourceMetric', message }] }
	});

	it('explains missing metrics and missing requests', () => {
		expect(autoscalerCause(hpa('unable to get metrics for resource cpu: no metrics returned from resource metrics API'))?.title).toBe(
			'web gets no metrics, so it does not scale'
		);
		expect(autoscalerCause(hpa('failed to get cpu utilization: missing request for cpu in container app'))).toMatchObject({
			title: 'web cannot scale: the pods set no CPU request',
			fix: 'requests'
		});
		expect(autoscalerCause(hpa('fine', 'True'))).toBeNull();
	});
});

describe('missingConfig', () => {
	it('reads the kubelet messages', () => {
		expect(missingConfig('configmap "settings" not found')).toEqual({ kind: 'ConfigMap', name: 'settings', key: null, keys: [] });
		expect(missingConfig("couldn't find key token in Secret shop/api-token")).toEqual({ kind: 'Secret', name: 'api-token', key: 'token', keys: ['token'] });
		expect(missingConfig('other')).toBeNull();
	});
});

describe('attentionTitle', () => {
	it('turns reasons into a headline', () => {
		expect(attentionTitle('web', ['FailedGetResourceMetric', 'FailedComputeMetricsReplicas'], '')).toBe('web gets no metrics');
		expect(attentionTitle('cart-1', ['Failed', 'BackOff'], 'Failed to pull image "busybox:x"')).toBe('cart-1 cannot pull its image');
		expect(attentionTitle('checkout-1', ['BackOff'], 'Back-off restarting failed container')).toBe('checkout-1 keeps restarting');
		expect(attentionTitle('batch-1', ['FailedScheduling'], '')).toBe('batch-1 cannot be placed on a node');
		expect(attentionTitle('config-1', ['Failed'], 'Error: secret "api-token" not found')).toBe(
			'config-1 needs the Secret api-token, which does not exist'
		);
		expect(attentionTitle('x', ['SomethingNew'], '')).toBe('x: SomethingNew');
	});
});

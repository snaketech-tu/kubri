/**
 * Why something does not run, in one plain sentence, with the one action that helps most. The
 * inspector, the logs of a container that never started and the overview's "Needs attention"
 * all say it the same way.
 */

/** The action a cause points to. */
export type CauseFix =
	/** Set image… on the workload that owns the pod. */
	| 'image'
	/** Raise memory limit… on the workload. */
	| 'memory'
	/** Change the CPU or memory requests of the workload. */
	| 'requests'
	/** Read the logs of the instance that ended. */
	| 'previousLogs'
	/** Create the Secret or ConfigMap the container needs. */
	| 'createConfig'
	/** The events say more than Kubri does. */
	| 'events';

export interface Cause {
	/** What is wrong, as one sentence without a final period. */
	title: string;
	/** What it means or what to check. */
	detail: string;
	fix: CauseFix | null;
	tone: 'bad' | 'warn';
	/** For `createConfig`: what is missing. */
	missing?: { kind: 'Secret' | 'ConfigMap'; name: string; key: string | null; keys: string[] };
	/** For `requests`: which resource the pod asks too much of. */
	resource?: 'cpu' | 'memory';
}

const PULL = /^(ErrImagePull|ImagePullBackOff|InvalidImageName|ErrImageNeverPull)$/;
/** How every container starts; not a problem. */
const STARTING = /^(ContainerCreating|PodInitializing)$/;

// Pods and autoscalers are read as Kubernetes sends them, hence `any`.

function statusOf(pod: any, container: string): any {
	const all = [...(pod?.status?.initContainerStatuses ?? []), ...(pod?.status?.containerStatuses ?? [])];
	return all.find((s: any) => s.name === container) ?? null;
}

function specOf(pod: any, container: string): any {
	const all = [...(pod?.spec?.initContainers ?? []), ...(pod?.spec?.containers ?? [])];
	return all.find((c: any) => c.name === container) ?? null;
}

/** The keys a container reads from a Secret or ConfigMap by name, through its environment. */
function keysOf(spec: any, kind: 'Secret' | 'ConfigMap', name: string): string[] {
	const field = kind === 'Secret' ? 'secretKeyRef' : 'configMapKeyRef';
	const keys = (spec?.env ?? []).flatMap((e: any) => (e.valueFrom?.[field]?.name === name ? [e.valueFrom[field].key] : []));
	return [...new Set<string>(keys)];
}

/** The Secret or ConfigMap a CreateContainerConfigError names, as the kubelet words it. */
export function missingConfig(message: string): Cause['missing'] | null {
	const object = /^(secret|configmap) "([^"]+)" not found/i.exec(message);
	if (object) return { kind: object[1].toLowerCase() === 'secret' ? 'Secret' : 'ConfigMap', name: object[2], key: null, keys: [] };
	const key = /couldn't find key (\S+) in (Secret|ConfigMap) (?:[^/\s]+\/)?(\S+)/i.exec(message);
	if (key) return { kind: key[2].toLowerCase() === 'secret' ? 'Secret' : 'ConfigMap', name: key[3], key: key[1], keys: [key[1]] };
	return null;
}

/**
 * Why `container` of `pod` does not run; null while it runs, has finished well, or is still
 * starting the normal way.
 */
export function containerCause(pod: any, container: string): Cause | null {
	const s = statusOf(pod, container);
	if (!s) return null;
	const waiting = s.state?.waiting;
	const terminated = s.state?.terminated;
	const last = s.lastState?.terminated;
	const reason: string = waiting?.reason ?? '';
	const message: string = waiting?.message ?? '';

	if (PULL.test(reason)) {
		const image = specOf(pod, container)?.image ?? s.image ?? '';
		return {
			title: `${container} never started: the image ${image} cannot be pulled`,
			detail:
				reason === 'InvalidImageName'
					? 'The image name is not valid. Set an image that exists.'
					: 'Check that the image and tag exist, that the nodes can reach the registry, and for a private registry, that a pull secret exists.',
			fix: 'image',
			tone: 'bad'
		};
	}
	if (reason === 'CreateContainerConfigError') {
		const missing = missingConfig(message);
		if (missing) {
			if (!missing.keys.length) missing.keys = keysOf(specOf(pod, container), missing.kind, missing.name);
			const what = missing.key ? `the key ${missing.key} in the ${missing.kind} ${missing.name}` : `the ${missing.kind} ${missing.name}`;
			return {
				title: `${container} never started: ${what} it needs does not exist`,
				detail: `Create it in this namespace, or change the pod so it no longer refers to it. Kubernetes starts the container once it is there.`,
				fix: 'createConfig',
				tone: 'bad',
				missing
			};
		}
		return { title: `${container} never started: its settings cannot be read`, detail: message, fix: 'events', tone: 'bad' };
	}
	if (/^(CreateContainerError|RunContainerError|StartError)$/.test(reason) || terminated?.reason === 'StartError') {
		return {
			title: `${container} could not be started`,
			detail: message || terminated?.message || 'The container runtime refused to start it; the events may say why.',
			fix: 'events',
			tone: 'bad'
		};
	}
	if (terminated?.reason === 'OOMKilled' || (reason === 'CrashLoopBackOff' && last?.reason === 'OOMKilled')) {
		const limit = specOf(pod, container)?.resources?.limits?.memory;
		return {
			title: `${container} ran out of memory and was stopped`,
			detail: limit
				? `It may use ${limit}, and needed more. Raise the limit, or find out why it uses that much.`
				: 'The node ran out of memory. Set a memory request and limit, so it is placed where it fits.',
			fix: 'memory',
			tone: 'bad'
		};
	}
	if (reason === 'CrashLoopBackOff') {
		const code = last?.exitCode;
		const restarts = s.restartCount ?? 0;
		return {
			title: `${container} keeps crashing`,
			detail: `It stopped${code !== undefined ? ` with exit code ${code}` : ''} and restarted ${restarts} ${restarts === 1 ? 'time' : 'times'}. Kubernetes waits before the next try. The previous instance's logs say why it stopped.`,
			fix: 'previousLogs',
			tone: 'bad'
		};
	}
	if (reason && !STARTING.test(reason)) {
		return { title: `${container} is waiting: ${reason}`, detail: message || 'The events may say why.', fix: 'events', tone: 'warn' };
	}
	return null;
}

/** The requests of the pod's containers for `resource`, such as "64 CPU" or "batch 2, sidecar 500m". */
function asked(pod: any, resource: 'cpu' | 'memory'): string | null {
	const requests: [string, string][] = (pod?.spec?.containers ?? []).flatMap((c: any) => {
		const value = c.resources?.requests?.[resource];
		return value ? [[c.name, String(value)]] : [];
	});
	if (!requests.length) return null;
	if (requests.length === 1) return resource === 'cpu' ? `${requests[0][1]} CPU` : `${requests[0][1]} of memory`;
	return requests.map(([name, value]) => `${name} ${value}`).join(', ');
}

/** Why a pod waits for a node or a volume, from the scheduler's condition; null if it does not. */
export function pendingCause(pod: any): Cause | null {
	if (pod?.status?.phase !== 'Pending') return null;
	const scheduled = (pod.status.conditions ?? []).find((c: any) => c.type === 'PodScheduled');
	if (!scheduled || scheduled.status !== 'False' || !scheduled.message) return null;
	const message: string = scheduled.message;
	for (const resource of ['cpu', 'memory'] as const) {
		if (!new RegExp(`Insufficient ${resource}`, 'i').test(message)) continue;
		const amount = asked(pod, resource);
		return {
			title: amount
				? `No node has room: the pod asks for ${amount}, more than any node has free`
				: `No node has room: the pod asks for more ${resource === 'cpu' ? 'CPU' : 'memory'} than any node has free`,
			detail: 'Lower the request, free room on a node, or add a node. "Why not each node?" shows what each node has left.',
			fix: 'requests',
			tone: 'warn',
			resource
		};
	}
	if (/untolerated taint|had taint/i.test(message)) {
		return {
			title: 'No node takes it: the nodes have taints the pod does not tolerate',
			detail: 'Add a toleration to the pod, or remove the taint from a node. "Why not each node?" names the taints.',
			fix: null,
			tone: 'warn'
		};
	}
	if (/didn't match (Pod's )?node (affinity|selector)/i.test(message)) {
		return {
			title: "No node matches the pod's node selector or affinity",
			detail: 'Label a node to match, or loosen the selector. "Why not each node?" shows which labels are missing.',
			fix: null,
			tone: 'warn'
		};
	}
	if (/unbound (immediate )?PersistentVolumeClaims|persistentvolumeclaim .* not found/i.test(message)) {
		return {
			title: 'Waiting for its volume: a PersistentVolumeClaim is not bound',
			detail: 'Check that the claim exists and that a storage class can provide it. The claim\'s events say more.',
			fix: 'events',
			tone: 'warn'
		};
	}
	return {
		title: 'Waiting for a node',
		detail: message.split(/(?<=\.)\s/)[0],
		fix: null,
		tone: 'warn'
	};
}

/** Why a HorizontalPodAutoscaler does not scale; null while it works. */
export function autoscalerCause(hpa: any): Cause | null {
	const active = (hpa?.status?.conditions ?? []).find((c: any) => c.type === 'ScalingActive');
	if (!active || active.status !== 'False') return null;
	const message: string = active.message ?? '';
	if (/missing request for (cpu|memory)/i.test(message)) {
		const resource = /missing request for (cpu|memory)/i.exec(message)![1].toLowerCase() as 'cpu' | 'memory';
		return {
			title: `${hpa.metadata.name} cannot scale: the pods set no ${resource === 'cpu' ? 'CPU' : 'memory'} request`,
			detail: `A target in percent is measured against the request. Set a ${resource === 'cpu' ? 'CPU' : 'memory'} request on the workload.`,
			fix: 'requests',
			tone: 'warn',
			resource
		};
	}
	if (/metrics|FailedGetResourceMetric/i.test(`${active.reason} ${message}`)) {
		return {
			title: `${hpa.metadata.name} gets no metrics, so it does not scale`,
			detail: /resource metrics API|metrics\.k8s\.io/i.test(message)
				? "Kubernetes' metrics API does not answer. Check that metrics-server runs. Without metrics the replica count stays as it is."
				: 'Without metrics the replica count stays as it is. The events say which metric is missing.',
			fix: 'events',
			tone: 'warn'
		};
	}
	return { title: `${hpa.metadata.name} does not scale: ${active.reason}`, detail: message, fix: 'events', tone: 'warn' };
}

const TITLES: [RegExp, (name: string) => string][] = [
	[/^(FailedGetResourceMetric|FailedComputeMetricsReplicas|FailedGetPodsMetric|FailedGetExternalMetric|FailedGetObjectMetric)$/, (n) => `${n} gets no metrics`],
	[/^BackOff$/, (n) => `${n} keeps restarting`],
	[/^(ErrImagePull|ImagePullBackOff|Failed)$/, (n) => `${n} has a container that fails`],
	[/^FailedScheduling$/, (n) => `${n} cannot be placed on a node`],
	[/^(Unhealthy|ProbeWarning)$/, (n) => `${n} fails its health checks`],
	[/^(FailedMount|FailedAttachVolume)$/, (n) => `${n} cannot mount a volume`],
	[/^(OOMKilling|OOMKilled)$/, (n) => `${n} ran out of memory`],
	[/^FailedCreate$/, (n) => `${n} cannot create its pods`],
	[/^(FailedKillPod|FailedPreStopHook|FailedPostStartHook)$/, (n) => `${n} has a container hook that fails`],
	[/^(NodeNotReady|NodeHasDiskPressure|NodeHasMemoryPressure|EvictionThresholdMet)$/, (n) => `${n} is short of resources`],
	[/^(Evicted)$/, (n) => `${n} was evicted from its node`],
	[/^(SyncLoadBalancerFailed)$/, (n) => `${n} gets no load balancer`]
];

/**
 * A plain headline for the warnings of one object, such as "web gets no metrics"; the raw
 * reasons stay below it. A pull failure says so when the message does.
 */
export function attentionTitle(name: string, reasons: string[], message: string): string {
	if (reasons.some((r) => /^(Failed|BackOff)$/.test(r)) && /pull(ing)? image|ErrImagePull|ImagePullBackOff/i.test(message)) {
		return `${name} cannot pull its image`;
	}
	const missing = /\b(secret|configmap) "([^"]+)" not found/i.exec(message);
	if (missing) return `${name} needs the ${missing[1].toLowerCase() === 'secret' ? 'Secret' : 'ConfigMap'} ${missing[2]}, which does not exist`;
	for (const reason of reasons) {
		const found = TITLES.find(([pattern]) => pattern.test(reason));
		if (found) return found[1](name);
	}
	return `${name}: ${reasons.join(', ')}`;
}

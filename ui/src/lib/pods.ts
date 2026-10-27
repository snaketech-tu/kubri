import type { KubeObject, LogSource, ResourceInfo, ResourceRef } from './api';

/** Kinds whose `spec.selector` picks the pods that logs and port-forwards use. */
const SELECTING_KINDS = ['Deployment', 'StatefulSet', 'DaemonSet', 'ReplicaSet', 'Job', 'ReplicationController', 'Service'];

function isCore(resource: ResourceRef, plural: string) {
	return resource.group === '' && resource.plural === plural;
}

export const isPods = (resource: ResourceRef) => isCore(resource, 'pods');
export const isNodes = (resource: ResourceRef) => isCore(resource, 'nodes');
export const isServices = (resource: ResourceRef) => isCore(resource, 'services');

/** Whether logs can be shown: pods, and objects that select pods, CRDs like Argo Rollouts included. */
export function hasLogs(resource: ResourceInfo, object?: KubeObject | null) {
	if (isPods(resource)) return true;
	if (!resource.namespaced) return false;
	if (object) return object.spec?.selector != null;
	return SELECTING_KINDS.includes(resource.kind);
}

export function canPortForward(resource: ResourceInfo) {
	return isPods(resource) || ['Deployment', 'StatefulSet', 'DaemonSet', 'ReplicaSet', 'Service'].includes(resource.kind);
}

function podSpec(object: KubeObject): Record<string, any> | undefined {
	return object.kind === 'Pod' ? object.spec : object.spec?.template?.spec;
}

/** Container names of a pod or a workload's pod template, init and debug containers last. */
export function containerNames(object: KubeObject | null | undefined): string[] {
	const spec = object && podSpec(object);
	if (!spec) return [];
	const names = (list: unknown) => (Array.isArray(list) ? list.map((c) => String(c.name)) : []);
	return [...names(spec.containers), ...names(spec.initContainers), ...names(spec.ephemeralContainers)];
}

export interface ForwardablePort {
	port: number;
	name: string | null;
	/** Where the port comes from: a container name, or the target port of a Service. */
	detail: string;
}

/** TCP ports that can be forwarded to: Service ports, or the container ports of a pod or workload. */
export function forwardablePorts(object: KubeObject | null | undefined): ForwardablePort[] {
	if (!object) return [];
	const tcp = (p: { protocol?: string }) => !p.protocol || p.protocol === 'TCP';
	if (object.kind === 'Service') {
		const ports: any[] = object.spec?.ports ?? [];
		return ports.filter(tcp).map((p) => ({
			port: p.port,
			name: p.name ?? null,
			detail: `targets ${p.targetPort ?? p.port}`
		}));
	}
	const spec = podSpec(object);
	const containers: any[] = spec?.containers ?? [];
	return containers.flatMap((c) =>
		(c.ports ?? []).filter(tcp).map((p: any) => ({ port: p.containerPort, name: p.name ?? null, detail: c.name }))
	);
}

/** Matches log lines: `/regex/flags`, or else a case-insensitive substring. */
export function logMatcher(filter: string): (text: string) => boolean {
	const f = filter.trim();
	if (!f) return () => true;
	const regex = /^\/(.+)\/([a-z]*)$/.exec(f);
	if (regex) {
		try {
			const re = new RegExp(regex[1], regex[2]);
			return (text) => re.test(text);
		} catch {
			// Not a valid regex yet, probably still being typed: match it literally.
		}
	}
	const needle = f.toLowerCase();
	return (text) => text.toLowerCase().includes(needle);
}

/** Removes terminal color and cursor codes. */
export function stripAnsi(text: string) {
	// eslint-disable-next-line no-control-regex
	return text.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, '');
}

/** The shortest label that tells the sources of a log apart. */
export function sourceLabels(sources: LogSource[]): string[] {
	const pods = new Set(sources.map((s) => s.pod));
	if (pods.size <= 1) return sources.map((s) => s.container);
	if (pods.size === sources.length) return sources.map((s) => s.pod);
	return sources.map((s) => `${s.pod}/${s.container}`);
}

/** A local port to suggest for `remote`: the same, unless it needs root. */
export function suggestedLocalPort(remote: number): number | null {
	return remote >= 1024 ? remote : null;
}

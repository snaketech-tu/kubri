import type { KubeObject, LogOptions, LogSource, ResourceInfo, ResourceRef } from './api';
import { plural, t } from './i18n';

/** Kinds whose `spec.selector` picks the pods that logs and port-forwards use. */
const SELECTING_KINDS = ['Deployment', 'StatefulSet', 'DaemonSet', 'ReplicaSet', 'Job', 'ReplicationController', 'Service'];

function isCore(resource: ResourceRef, name: string) {
	return resource.group === '' && resource.plural === name;
}

export const isPods = (resource: ResourceRef) => isCore(resource, 'pods');
export const isNodes = (resource: ResourceRef) => isCore(resource, 'nodes');
export const isServices = (resource: ResourceRef) => isCore(resource, 'services');
export const isNamespaces = (resource: ResourceRef) => isCore(resource, 'namespaces');
export const isSecrets = (resource: ResourceRef) => isCore(resource, 'secrets');
export const isCronJobs = (resource: ResourceRef) => resource.group === 'batch' && resource.plural === 'cronjobs';
export const isReplicaSets = (resource: ResourceRef) => resource.group === 'apps' && resource.plural === 'replicasets';
export const isCRDs = (resource: ResourceRef) =>
	resource.group === 'apiextensions.k8s.io' && resource.plural === 'customresourcedefinitions';

/** Kinds k9s finds the users of with `u`: the pod specs that name them, and the routes to a Service. */
export function hasUsers(resource: ResourceRef) {
	return (
		(resource.group === '' && ['configmaps', 'secrets', 'persistentvolumeclaims', 'serviceaccounts', 'services'].includes(resource.plural)) ||
		(resource.group === 'scheduling.k8s.io' && resource.plural === 'priorityclasses')
	);
}

/** Kinds whose container images can be set, as in k9s. */
export function hasImages(resource: ResourceRef) {
	return (
		isPods(resource) ||
		(resource.group === 'apps' && ['deployments', 'statefulsets', 'daemonsets'].includes(resource.plural))
	);
}

/** A pod's or pod template's containers with their images, init containers marked. */
export function containerImages(object: KubeObject | null | undefined): { name: string; image: string; init: boolean }[] {
	const spec = object && podSpec(object);
	if (!spec) return [];
	const list = (items: unknown, init: boolean) =>
		Array.isArray(items) ? items.map((c) => ({ name: String(c.name), image: String(c.image ?? ''), init })) : [];
	return [...list(spec.containers, false), ...list(spec.initContainers, true)];
}

/** A port the `k9scli.io/port-forwards` annotation asks for. */
export interface AnnotatedPort {
	container: string | null;
	local: number | null;
	/** A port number, or a port name to look up. */
	port: number | string;
}

/**
 * The ports of a `k9scli.io/port-forwards` or `k9scli.io/auto-port-forwards` annotation, like
 * `8080`, `9090:http` or `nginx::8080:80`, as k9s reads them: an optional container, an
 * optional local port, then a container port by number or name.
 */
export function annotatedPorts(annotation: string | undefined): AnnotatedPort[] {
	if (!annotation) return [];
	return annotation.split(',').flatMap((entry): AnnotatedPort[] => {
		const m = /^(?:([\w-]+)::)?(?:(\d*):)?([\w-]+)$/.exec(entry.trim());
		if (!m) return [];
		const [, container, local, port] = m;
		return [
			{
				container: container ?? null,
				local: local ? Number(local) : null,
				port: /^\d+$/.test(port) ? Number(port) : port
			}
		];
	});
}

export const PORT_FORWARDS_KEY = 'k9scli.io/port-forwards';
export const AUTO_PORT_FORWARDS_KEY = 'k9scli.io/auto-port-forwards';

/**
 * The forwards an annotation of a pod or pod template asks for, with port names looked up in
 * the containers. Services are left out: k9s reads the annotations of pods.
 */
export function annotatedForwards(object: KubeObject | null | undefined, key: string): { remote: number; local: number | null }[] {
	if (!object || object.kind === 'Service') return [];
	const annotations = object.kind === 'Pod' ? object.metadata.annotations : object.spec?.template?.metadata?.annotations;
	const declared = forwardablePorts(object);
	return annotatedPorts(annotations?.[key]).flatMap((a) => {
		const remote =
			typeof a.port === 'number'
				? a.port
				: declared.find((p) => p.name === a.port && (!a.container || p.detail === a.container))?.port;
		return remote ? [{ remote, local: a.local }] : [];
	});
}

/** The port a forward dialog starts with: the one the annotation names, as in k9s, else the first declared. */
export function startingPort(object: KubeObject | null | undefined): { remote: number; local: number | null } | null {
	const [wanted] = annotatedForwards(object, PORT_FORWARDS_KEY);
	if (wanted) return { remote: wanted.remote, local: wanted.local ?? suggestedLocalPort(wanted.remote) };
	const [first] = forwardablePorts(object);
	return first ? { remote: first.port, local: suggestedLocalPort(first.port) } : null;
}

/**
 * The ports a forward dialog shows once the object's ports are known: the starting port, but a
 * port typed in while they loaded stays, and a typed container port is not paired with another's
 * local port.
 */
export function portsOnceLoaded(
	start: { remote: number; local: number | null } | null,
	typed: { remote: number | null; local: number | null }
): { remote: number | null; local: number | null } {
	const remote = typed.remote ?? start?.remote ?? null;
	const local = typed.local ?? (typed.remote === null ? (start?.local ?? null) : null);
	return { remote, local };
}

/** A value of a Secret's data: its size, and its text when it is UTF-8. */
export function secretValue(base64: string): { bytes: number; text: string | null } {
	let raw: string;
	try {
		raw = atob(base64);
	} catch {
		return { bytes: 0, text: null };
	}
	const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
	try {
		return { bytes: bytes.length, text: new TextDecoder('utf-8', { fatal: true }).decode(bytes) };
	} catch {
		return { bytes: bytes.length, text: null };
	}
}

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
			detail: t('targets {port}', { port: p.targetPort ?? p.port })
		}));
	}
	const spec = podSpec(object);
	const containers: any[] = spec?.containers ?? [];
	return containers.flatMap((c) =>
		(c.ports ?? []).filter(tcp).map((p: any) => ({ port: p.containerPort, name: p.name ?? null, detail: c.name }))
	);
}

export interface LogRange {
	label: string;
	options: Pick<LogOptions, 'tailLines' | 'sinceSeconds'>;
}

/**
 * The ranges the log view offers, in order, and the one to start with: the k9s `logger` tail,
 * or its `sinceSeconds` when that is set.
 */
export function logRanges(tail: number, sinceSeconds: number): { ranges: Map<string, LogRange>; start: string } {
	const ranges = new Map<string, LogRange>([['tail', { label: plural(tail, 'Last line', 'Last {n} lines'), options: { tailLines: tail } }]]);
	for (const n of [500, 2000]) {
		if (n !== tail) ranges.set(String(n), { label: plural(n, 'Last line', 'Last {n} lines'), options: { tailLines: n } });
	}
	const times: [string, number][] = [
		['1m', 60],
		['5m', 300],
		['15m', 900],
		['30m', 1800],
		['1h', 3600]
	];
	for (const [key, seconds] of times) ranges.set(key, { label: last(seconds), options: { sinceSeconds: seconds } });
	let start = 'tail';
	if (sinceSeconds > 0) {
		start = times.find(([, s]) => s === sinceSeconds)?.[0] ?? 'since';
		if (start === 'since') ranges.set(start, { label: last(sinceSeconds), options: { sinceSeconds } });
	}
	ranges.set('all', { label: t('Everything'), options: {} });
	return { ranges, start };
}

/** "Last 5 minutes", in the largest whole unit. */
function last(seconds: number) {
	if (seconds % 3600 === 0) return plural(seconds / 3600, 'Last hour', 'Last {n} hours');
	if (seconds % 60 === 0) return plural(seconds / 60, 'Last minute', 'Last {n} minutes');
	return plural(seconds, 'Last second', 'Last {n} seconds');
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

/** The registry an image is pulled from, as the container runtime reads its name. */
export function registryOf(image: string): string {
	const first = image.split('/')[0];
	const hasHost = image.includes('/') && (first.includes('.') || first.includes(':') || first === 'localhost');
	return hasHost ? first : 'docker.io';
}

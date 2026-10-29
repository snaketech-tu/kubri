import { parseAllDocuments, stringify } from 'yaml';
import type { ResourceInfo } from './api';
import { resolve, type KindSchema, type SchemaNode } from './schema';

/** What a template or a short form like `:new deploy web --image nginx` fills in. */
export interface TemplateParams {
	name?: string;
	namespace?: string;
	image?: string;
	replicas?: number;
	port?: number;
	/** The port a Service sends traffic to, when it differs from `port`. */
	targetPort?: number;
	/** `ClusterIP`, `NodePort`, `LoadBalancer`. */
	serviceType?: string;
	schedule?: string;
	command?: string[];
	/** ConfigMap and Secret entries, `--from-literal`. */
	data?: Record<string, string>;
}

/** Kinds with a template written for them; the rest come from their schema. */
export const TEMPLATE_KINDS = [
	'Deployment',
	'Service',
	'ConfigMap',
	'Secret',
	'Job',
	'CronJob',
	'Ingress',
	'StatefulSet',
	'DaemonSet',
	'Pod',
	'PersistentVolumeClaim',
	'ServiceAccount',
	'Namespace'
] as const;

/** The groups of the kinds with a template, to tell them from custom kinds of the same name. */
export const CORE_GROUPS = ['', 'apps', 'batch', 'networking.k8s.io'];

const API_VERSIONS: Record<string, string> = {
	Deployment: 'apps/v1',
	StatefulSet: 'apps/v1',
	DaemonSet: 'apps/v1',
	Job: 'batch/v1',
	CronJob: 'batch/v1',
	Ingress: 'networking.k8s.io/v1',
	Service: 'v1',
	ConfigMap: 'v1',
	Secret: 'v1',
	Pod: 'v1',
	PersistentVolumeClaim: 'v1',
	ServiceAccount: 'v1',
	Namespace: 'v1'
};

const CLUSTER_SCOPED = new Set(['Namespace']);

function metadata(kind: string, p: TemplateParams, fallback: string) {
	const name = p.name || fallback;
	return CLUSTER_SCOPED.has(kind) || !p.namespace ? { name } : { name, namespace: p.namespace };
}

function container(p: TemplateParams, name: string) {
	return {
		name,
		image: p.image || 'nginx:1.27',
		...(p.command?.length ? { command: p.command } : {}),
		...(p.port ? { ports: [{ containerPort: p.port }] } : {})
	};
}

/** A pod template for workloads, labelled `app: <name>`. */
function podTemplate(p: TemplateParams, name: string, restartPolicy?: string) {
	return {
		metadata: { labels: { app: name } },
		spec: { ...(restartPolicy ? { restartPolicy } : {}), containers: [container(p, name)] }
	};
}

/** An object for one of `TEMPLATE_KINDS`, or null for other kinds. */
export function templateObject(kind: string, p: TemplateParams = {}): Record<string, unknown> | null {
	const apiVersion = API_VERSIONS[kind];
	if (!apiVersion) return null;
	const fallback = kind === 'Namespace' ? 'my-namespace' : `my-${kind.toLowerCase()}`;
	const meta = metadata(kind, p, fallback);
	const name = meta.name;
	const head = { apiVersion, kind, metadata: meta };
	const selector = { matchLabels: { app: name } };
	switch (kind) {
		case 'Deployment':
			return { ...head, spec: { replicas: p.replicas ?? 1, selector, template: podTemplate(p, name) } };
		case 'StatefulSet':
			return {
				...head,
				spec: { serviceName: name, replicas: p.replicas ?? 1, selector, template: podTemplate(p, name) }
			};
		case 'DaemonSet':
			return { ...head, spec: { selector, template: podTemplate(p, name) } };
		case 'Pod':
			return { ...head, metadata: { ...meta, labels: { app: name } }, spec: { containers: [container(p, name)] } };
		case 'Job':
			return { ...head, spec: { backoffLimit: 2, template: podTemplate(p, name, 'Never') } };
		case 'CronJob':
			return {
				...head,
				spec: {
					schedule: p.schedule || '0 * * * *',
					jobTemplate: { spec: { backoffLimit: 2, template: podTemplate(p, name, 'OnFailure') } }
				}
			};
		case 'Service': {
			const port = p.port ?? 80;
			return {
				...head,
				spec: {
					...(p.serviceType && p.serviceType !== 'ClusterIP' ? { type: p.serviceType } : {}),
					selector: { app: name },
					ports: [{ name: 'http', port, targetPort: p.targetPort ?? port }]
				}
			};
		}
		case 'Ingress':
			return {
				...head,
				spec: {
					rules: [
						{
							host: `${name}.example.com`,
							http: {
								paths: [
									{
										path: '/',
										pathType: 'Prefix',
										backend: { service: { name, port: { number: p.port ?? 80 } } }
									}
								]
							}
						}
					]
				}
			};
		case 'ConfigMap':
			return { ...head, data: p.data ?? { key: 'value' } };
		case 'Secret':
			// `stringData` takes plain text; the API server stores it base64-encoded in `data`.
			return { ...head, type: 'Opaque', stringData: p.data ?? { password: 'change-me' } };
		case 'PersistentVolumeClaim':
			return { ...head, spec: { accessModes: ['ReadWriteOnce'], resources: { requests: { storage: '1Gi' } } } };
		case 'ServiceAccount':
		case 'Namespace':
			return head;
	}
	return null;
}

/** A placeholder for a field the schema requires. */
function placeholder(schema: KindSchema, node: SchemaNode | undefined, depth: number): unknown {
	const resolved = resolve(schema, node);
	if (!resolved) return '';
	if (resolved.default !== undefined) return resolved.default;
	if (resolved.enum?.length) return resolved.enum[0];
	if (resolved['x-kubernetes-int-or-string']) return '';
	switch (resolved.type) {
		case 'integer':
		case 'number':
			return 0;
		case 'boolean':
			return false;
		case 'array': {
			const item = placeholder(schema, resolved.items, depth + 1);
			return item && typeof item === 'object' && Object.keys(item).length ? [item] : [];
		}
		case 'string':
			return '';
	}
	return required(schema, resolved, depth + 1);
}

/** The fields an object must have, filled with placeholders, a few levels deep. */
function required(schema: KindSchema, node: SchemaNode, depth: number): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	if (depth > 4) return out;
	for (const key of node.required ?? []) {
		if (['apiVersion', 'kind', 'metadata', 'status'].includes(key)) continue;
		out[key] = placeholder(schema, node.properties?.[key], depth);
	}
	return out;
}

/** An object for any kind, with what its schema requires; a `spec` stays even when empty, if the kind has one. */
export function skeleton(resource: ResourceInfo, schema: KindSchema | null, p: TemplateParams = {}) {
	const apiVersion = resource.group ? `${resource.group}/${resource.version}` : resource.version;
	const name = p.name || `my-${resource.kind.toLowerCase()}`;
	const head = {
		apiVersion,
		kind: resource.kind,
		metadata: resource.namespaced && p.namespace ? { name, namespace: p.namespace } : { name }
	};
	const root = schema && resolve(schema, schema.schemas[schema.root]);
	if (!schema || !root) return { ...head, spec: {} };
	const body = required(schema, root, 0);
	if (root.properties?.spec && !('spec' in body)) {
		const spec = resolve(schema, root.properties.spec);
		body.spec = spec ? required(schema, spec, 1) : {};
	}
	return { ...head, ...body };
}

export function toYaml(objects: unknown[]): string {
	return objects.map((o) => stringify(o, { lineWidth: 0 })).join('---\n');
}

/** The name and first container port of the last workload among `objects`, for a Service or Ingress to point at. */
export function workloadOf(objects: unknown[]): TemplateParams {
	for (const o of [...objects].reverse() as any[]) {
		if (!['Deployment', 'StatefulSet', 'DaemonSet'].includes(o?.kind) || !o.metadata?.name) continue;
		const port = o.spec?.template?.spec?.containers?.[0]?.ports?.[0]?.containerPort;
		return { name: o.metadata.name, ...(typeof port === 'number' ? { port } : {}) };
	}
	return {};
}

/** Annotations the API server or controllers write, which a copy must not carry. */
const SERVER_ANNOTATIONS = [
	'kubectl.kubernetes.io/last-applied-configuration',
	'deployment.kubernetes.io/revision',
	'meta.helm.sh/release-name',
	'meta.helm.sh/release-namespace'
];
/** Labels controllers set to tie objects to their owner, which would confuse them in a copy. */
const SERVER_LABELS = [
	'controller-uid',
	'batch.kubernetes.io/controller-uid',
	'pod-template-hash',
	'job-name',
	'batch.kubernetes.io/job-name'
];

/** Drops `keys` from the map at `parent[key]`, and the map if it ends up empty. */
function dropKeys(parent: Record<string, any> | undefined, key: string, keys: string[]) {
	const map = parent?.[key];
	if (!map || typeof map !== 'object') return;
	for (const k of keys) delete map[k];
	if (!Object.keys(map).length) delete parent![key];
}

/**
 * A copy of `object` to create elsewhere, as `kubectl get -o yaml` would need editing for:
 * without status, what the API server assigns (uid, versions, cluster IPs, node, volume) and
 * what ties it to its owner. The namespace is left out, for the target to set.
 */
export function cloneable(object: Record<string, any>, name?: string): Record<string, any> {
	const copy = structuredClone(object);
	delete copy.status;
	const meta = copy.metadata ?? {};
	for (const key of [
		'uid',
		'resourceVersion',
		'generation',
		'creationTimestamp',
		'deletionTimestamp',
		'managedFields',
		'ownerReferences',
		'selfLink',
		'namespace',
		'finalizers'
	]) {
		delete meta[key];
	}
	if (name) meta.name = name;
	dropKeys(meta, 'annotations', SERVER_ANNOTATIONS);
	dropKeys(meta, 'labels', SERVER_LABELS);
	const spec = copy.spec;
	if (spec && typeof spec === 'object') {
		if (copy.kind === 'Service') {
			for (const key of ['clusterIP', 'clusterIPs']) delete spec[key];
			for (const port of spec.ports ?? []) delete port.nodePort;
		}
		if (copy.kind === 'Pod') delete spec.nodeName;
		if (copy.kind === 'PersistentVolumeClaim') delete spec.volumeName;
		if (copy.kind === 'Job') {
			// The API server writes a selector of its own uid, and the copy gets a new one.
			delete spec.selector;
			dropKeys(spec.template?.metadata, 'labels', SERVER_LABELS);
		}
	}
	return copy;
}

/** A short form like kubectl create, after `:new`: the kind as typed, then the object. */
export interface ShortForm {
	kind: string;
	params: TemplateParams;
}

const SERVICE_TYPES: Record<string, string> = {
	clusterip: 'ClusterIP',
	nodeport: 'NodePort',
	loadbalancer: 'LoadBalancer'
};

/**
 * Reads `deploy web --image nginx --replicas 3`, `svc nodeport web --tcp 80:8080`,
 * `cm settings --from-literal mode=fast`, `secret generic db --from-literal password=x`,
 * `cronjob nightly --image busybox --schedule "0 3 * * *" -- sh -c date` and `-n shop`.
 * Flags take `--flag value` or `--flag=value`.
 */
export function parseShortForm(input: string): ShortForm | null {
	const words = splitWords(input);
	const kind = words.shift();
	if (!kind) return null;
	const params: TemplateParams = {};
	const k = kind.toLowerCase();
	if (['svc', 'service', 'services'].includes(k) && SERVICE_TYPES[words[0]?.toLowerCase()]) {
		params.serviceType = SERVICE_TYPES[words.shift()!.toLowerCase()];
	}
	if (['secret', 'secrets'].includes(k) && words[0] === 'generic') words.shift();
	while (words.length) {
		const word = words.shift()!;
		if (word === '--') {
			params.command = words.splice(0);
			break;
		}
		if (!word.startsWith('-')) {
			params.name ??= word;
			continue;
		}
		const [flag, inline] = word.split(/=(.*)/s, 2);
		const value = () => inline ?? words.shift() ?? '';
		switch (flag) {
			case '--image':
				params.image = value();
				break;
			case '--replicas':
			case '-r':
				params.replicas = Number(value()) || 0;
				break;
			case '--port':
				params.port = Number(value()) || undefined;
				break;
			case '--tcp': {
				const [port, target] = value().split(':').map(Number);
				params.port = port || undefined;
				if (target) params.targetPort = target;
				break;
			}
			case '--schedule':
				params.schedule = value();
				break;
			case '--from-literal': {
				const [key, ...rest] = value().split('=');
				if (key) params.data = { ...params.data, [key]: rest.join('=') };
				break;
			}
			case '-n':
			case '--namespace':
				params.namespace = value();
				break;
		}
	}
	return { kind, params };
}

/** Splits like a shell: quotes keep spaces. */
export function splitWords(input: string): string[] {
	const words: string[] = [];
	for (const match of input.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)) words.push(match[1] ?? match[2] ?? match[3]);
	return words;
}

/** The objects in a YAML text, or where it does not parse. Empty documents are left out. */
export function parseObjects(text: string): { objects: unknown[]; error: string | null } {
	const objects: unknown[] = [];
	for (const doc of parseAllDocuments(text)) {
		if (!('errors' in doc)) continue;
		const problem = doc.errors[0];
		if (problem) {
			const line = problem.linePos?.[0]?.line;
			return { objects: [], error: line ? `Line ${line}: ${problem.message.split('\n')[0]}` : problem.message };
		}
		const value = doc.toJS();
		if (value == null) continue;
		// A `List`, as `kubectl get -o yaml` writes, stands for its items.
		if (typeof value === 'object' && value.kind === 'List' && Array.isArray(value.items)) objects.push(...value.items);
		else objects.push(value);
	}
	return { objects, error: null };
}

import type { Reference } from './api';

/** `3 Deployments`, `1 Ingress`: a count with the kind, plural when needed. */
export function counted(n: number, kind: string): string {
	return `${n} ${n === 1 ? kind : kind.endsWith('s') ? `${kind}es` : `${kind}s`}`;
}

/** What uses an object, counted by kind: `3 Deployments, 1 Pod and 1 Ingress for TLS`. */
export function summarizeUsers(refs: Reference[]): string {
	const counts = new Map<string, { kind: string; how?: string; n: number }>();
	for (const r of refs) {
		const key = `${r.kind}/${r.how ?? ''}`;
		const entry = counts.get(key) ?? { kind: r.kind, how: r.how, n: 0 };
		entry.n++;
		counts.set(key, entry);
	}
	const parts = [...counts.values()].map((c) => `${counted(c.n, c.kind)}${c.how ? ` for ${c.how}` : ''}`);
	return parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}` : (parts[0] ?? '');
}

export interface Related {
	kind: string;
	apiVersion: string;
	name: string;
	/** How the object uses it: `volume`, `environment`, `service account`. */
	how: string;
}

interface PodSpecLike {
	serviceAccountName?: string;
	volumes?: { configMap?: { name?: string }; secret?: { secretName?: string }; persistentVolumeClaim?: { claimName?: string }; projected?: { sources?: { configMap?: { name?: string }; secret?: { name?: string } }[] } }[];
	containers?: ContainerLike[];
	initContainers?: ContainerLike[];
}

interface ContainerLike {
	envFrom?: { configMapRef?: { name?: string }; secretRef?: { name?: string } }[];
	env?: { valueFrom?: { configMapKeyRef?: { name?: string }; secretKeyRef?: { name?: string } } }[];
}

/**
 * The ConfigMaps, Secrets, volume claims and service account a pod, or a workload's pod template,
 * names in its spec, each once.
 */
export function relatedOf(object: { kind?: string; spec?: unknown }): Related[] {
	const spec = (object.kind === 'Pod' ? object.spec : (object.spec as { template?: { spec?: unknown } } | undefined)?.template?.spec) as PodSpecLike | undefined;
	if (!spec) return [];
	const found = new Map<string, Related>();
	const add = (kind: string, apiVersion: string, name: string | undefined, how: string) => {
		if (name && !found.has(`${kind}/${name}`)) found.set(`${kind}/${name}`, { kind, apiVersion, name, how });
	};
	for (const v of spec.volumes ?? []) {
		add('ConfigMap', 'v1', v.configMap?.name, 'volume');
		add('Secret', 'v1', v.secret?.secretName, 'volume');
		add('PersistentVolumeClaim', 'v1', v.persistentVolumeClaim?.claimName, 'volume');
		for (const s of v.projected?.sources ?? []) {
			add('ConfigMap', 'v1', s.configMap?.name, 'volume');
			add('Secret', 'v1', s.secret?.name, 'volume');
		}
	}
	for (const c of [...(spec.containers ?? []), ...(spec.initContainers ?? [])]) {
		for (const e of c.envFrom ?? []) {
			add('ConfigMap', 'v1', e.configMapRef?.name, 'environment');
			add('Secret', 'v1', e.secretRef?.name, 'environment');
		}
		for (const e of c.env ?? []) {
			add('ConfigMap', 'v1', e.valueFrom?.configMapKeyRef?.name, 'environment');
			add('Secret', 'v1', e.valueFrom?.secretKeyRef?.name, 'environment');
		}
	}
	if (spec.serviceAccountName && spec.serviceAccountName !== 'default') {
		add('ServiceAccount', 'v1', spec.serviceAccountName, 'service account');
	}
	return [...found.values()];
}

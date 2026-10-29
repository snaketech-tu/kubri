/** Metadata every copy of an object has its own of, so it says nothing in a comparison. */
const OWN_METADATA = ['uid', 'resourceVersion', 'creationTimestamp', 'generation', 'managedFields', 'selfLink'];
/** Annotations tools write about this copy's history. */
const OWN_ANNOTATIONS = [
	'kubectl.kubernetes.io/last-applied-configuration',
	'deployment.kubernetes.io/revision',
	'kubectl.kubernetes.io/restartedAt'
];

type Json = Record<string, unknown>;
const isObject = (value: unknown): value is Json => !!value && typeof value === 'object' && !Array.isArray(value);

/** An object as it compares with its copy elsewhere: without what each copy has of its own. */
export function comparable(object: unknown, status = false): Json {
	if (!isObject(object)) return {};
	// A JSON copy: objects held in Svelte state are proxies, which structuredClone refuses.
	const copy = JSON.parse(JSON.stringify(object)) as Json;
	if (!status) delete copy.status;
	const metadata = copy.metadata;
	if (isObject(metadata)) {
		for (const key of OWN_METADATA) delete metadata[key];
		delete metadata.namespace;
		if (isObject(metadata.annotations)) {
			for (const key of OWN_ANNOTATIONS) delete metadata.annotations[key];
			if (!Object.keys(metadata.annotations).length) delete metadata.annotations;
		}
	}
	// The pod template's restart stamp differs after every rollout restart.
	const template = (copy.spec as Json | undefined)?.template as Json | undefined;
	const annotations = (template?.metadata as Json | undefined)?.annotations;
	if (isObject(annotations)) {
		delete annotations['kubectl.kubernetes.io/restartedAt'];
		if (!Object.keys(annotations).length) delete (template!.metadata as Json).annotations;
	}
	return copy;
}

/** Container images by container, from a pod or any object with a pod template. */
export function images(object: unknown): Map<string, string> {
	const found = new Map<string, string>();
	if (!isObject(object)) return found;
	const spec = object.spec as Json | undefined;
	const pod = (((spec?.jobTemplate as Json | undefined)?.spec as Json | undefined)?.template ?? spec?.template) as Json | undefined;
	const podSpec = (pod?.spec ?? (object.kind === 'Pod' ? spec : undefined)) as Json | undefined;
	for (const key of ['initContainers', 'containers']) {
		for (const c of (podSpec?.[key] as { name: string; image?: string }[] | undefined) ?? []) {
			if (c.image) found.set(c.name, c.image);
		}
	}
	return found;
}

/** Containers whose image differs between two copies; a missing container has no image. */
export function imageChanges(left: unknown, right: unknown): { container: string; left: string | null; right: string | null }[] {
	const a = images(left);
	const b = images(right);
	const names = [...new Set([...a.keys(), ...b.keys()])];
	return names.filter((n) => a.get(n) !== b.get(n)).map((n) => ({ container: n, left: a.get(n) ?? null, right: b.get(n) ?? null }));
}

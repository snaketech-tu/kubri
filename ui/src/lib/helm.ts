import { parseAllDocuments } from 'yaml';
import type { Health, HelmRelease, ResourceInfo } from './api';

/** How a release status reads at a glance; superseded and uninstalled revisions are history, not health. */
export function statusHealth(status: string): Health | null {
	if (status === 'deployed') return 'ok';
	if (status === 'failed') return 'failed';
	if (status.startsWith('pending') || status === 'uninstalling') return 'warning';
	return null;
}

/** Keeps releases whose name, namespace, chart or status contain `text`. */
export function filterReleases(releases: HelmRelease[], text: string) {
	const t = text.trim().toLowerCase();
	if (!t) return releases;
	return releases.filter((r) =>
		[r.name, r.namespace, r.chart, r.chartVersion, r.status].some((v) => v.toLowerCase().includes(t))
	);
}

export function chartLabel(r: Pick<HelmRelease, 'chart' | 'chartVersion'>) {
	return r.chartVersion ? `${r.chart}-${r.chartVersion}` : r.chart;
}

export function releaseKey(r: Pick<HelmRelease, 'namespace' | 'name'>) {
	return `${r.namespace}/${r.name}`;
}

/** Whether values hold anything worth showing; `{}` and null mean the chart defaults apply. */
export function hasValues(values: unknown) {
	if (values === null || values === undefined) return false;
	return typeof values !== 'object' || Object.keys(values).length > 0;
}

/** The revision `helm rollback` goes back to without one: the newest older than the current. */
export function previousRevision(history: HelmRelease[], current: number): number | null {
	const older = history.map((r) => r.revision).filter((n) => n < current);
	return older.length ? Math.max(...older) : null;
}

export interface ManifestObject {
	apiVersion: string;
	kind: string;
	name: string;
	/** As written in the manifest; namespaced objects without one land in the release namespace. */
	namespace: string | null;
}

/** The objects a rendered manifest creates, in manifest order. Unparsable documents are skipped. */
export function manifestObjects(manifest: string): ManifestObject[] {
	const objects: ManifestObject[] = [];
	for (const doc of parseAllDocuments(manifest)) {
		if (doc.errors.length) continue;
		const value = doc.toJS() as { apiVersion?: unknown; kind?: unknown; metadata?: { name?: unknown; namespace?: unknown } };
		if (!value || typeof value.kind !== 'string' || typeof value.metadata?.name !== 'string') continue;
		objects.push({
			apiVersion: typeof value.apiVersion === 'string' ? value.apiVersion : '',
			kind: value.kind,
			name: value.metadata.name,
			namespace: typeof value.metadata.namespace === 'string' ? value.metadata.namespace : null
		});
	}
	return objects;
}

/** The served resource type of a manifest object, so it can be opened. */
export function manifestResource(resources: ResourceInfo[], object: ManifestObject) {
	const group = object.apiVersion.includes('/') ? object.apiVersion.split('/')[0] : '';
	return resources.find((r) => r.group === group && r.kind === object.kind);
}

type Json = Record<string, unknown>;

/** The objects of a rendered manifest in full, in manifest order. */
export function manifestDocuments(manifest: string): Json[] {
	const docs: Json[] = [];
	for (const doc of parseAllDocuments(manifest)) {
		if (doc.errors.length) continue;
		const value = doc.toJS();
		if (value && typeof value === 'object' && !Array.isArray(value) && typeof value.kind === 'string') docs.push(value);
	}
	return docs;
}

export type ManifestChangeKind = 'added' | 'removed' | 'changed' | 'unchanged';

export interface ManifestChange {
	kind: string;
	name: string;
	namespace: string | null;
	change: ManifestChangeKind;
	before: Json | null;
	after: Json | null;
}

function identity(o: Json, namespace: string) {
	const meta = (o.metadata ?? {}) as { name?: unknown; namespace?: unknown };
	const group = String(o.apiVersion ?? '').includes('/') ? String(o.apiVersion).split('/')[0] : '';
	return {
		kind: String(o.kind),
		name: typeof meta.name === 'string' ? meta.name : '',
		namespace: typeof meta.namespace === 'string' ? meta.namespace : null,
		// The version may change between charts while the object stays the same.
		key: `${group}/${o.kind}/${typeof meta.namespace === 'string' ? meta.namespace : namespace}/${meta.name}`
	};
}

function same(a: unknown, b: unknown): boolean {
	return JSON.stringify(sorted(a)) === JSON.stringify(sorted(b));
}

function sorted(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(sorted);
	if (value && typeof value === 'object') {
		return Object.fromEntries(
			Object.keys(value)
				.sort()
				.map((k) => [k, sorted((value as Json)[k])])
		);
	}
	return value;
}

const ORDER: Record<ManifestChangeKind, number> = { changed: 0, added: 1, removed: 2, unchanged: 3 };

/**
 * What changes between two rendered manifests of a release, object by object: changes first,
 * then additions and removals, each in manifest order.
 */
export function manifestChanges(before: string, after: string, namespace: string): ManifestChange[] {
	const old = new Map(manifestDocuments(before).map((o) => [identity(o, namespace).key, o]));
	const changes: ManifestChange[] = [];
	for (const o of manifestDocuments(after)) {
		const id = identity(o, namespace);
		const was = old.get(id.key) ?? null;
		old.delete(id.key);
		const change = !was ? 'added' : same(was, o) ? 'unchanged' : 'changed';
		changes.push({ kind: id.kind, name: id.name, namespace: id.namespace, change, before: was, after: o });
	}
	for (const o of old.values()) {
		const id = identity(o, namespace);
		changes.push({ kind: id.kind, name: id.name, namespace: id.namespace, change: 'removed', before: o, after: null });
	}
	return changes
		.map((c, i) => ({ c, i }))
		.sort((a, b) => ORDER[a.c.change] - ORDER[b.c.change] || a.i - b.i)
		.map(({ c }) => c);
}

/** Whether two sets of values differ. */
export function valuesDiffer(a: unknown, b: unknown) {
	return !same(hasValues(a) ? a : {}, hasValues(b) ? b : {});
}

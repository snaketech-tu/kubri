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

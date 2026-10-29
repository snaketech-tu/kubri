import type { ColumnSettings, ResourceInfo } from './api';

export const NO_COLUMNS: ColumnSettings = { hidden: [], shown: [], order: [], custom: [] };

const custom = (...columns: [string, string][]): ColumnSettings => ({
	...NO_COLUMNS,
	custom: columns.map(([name, path]) => ({ name, path }))
});
const SUSPENDED: [string, string] = ['Suspended', '.spec.suspend'];
const ARTIFACT = custom(['Revision', '.status.artifact.revision'], SUSPENDED);

/** Columns kinds get until the settings say otherwise: what Flux and Argo CD leave out. */
const BUILTIN: Record<string, ColumnSettings> = {
	'kustomize.toolkit.fluxcd.io/Kustomization': custom(['Revision', '.status.lastAppliedRevision'], SUSPENDED),
	'helm.toolkit.fluxcd.io/HelmRelease': custom(['Revision', '.status.lastAttemptedRevision'], SUSPENDED),
	'source.toolkit.fluxcd.io/GitRepository': ARTIFACT,
	'source.toolkit.fluxcd.io/OCIRepository': ARTIFACT,
	'source.toolkit.fluxcd.io/HelmRepository': ARTIFACT,
	'source.toolkit.fluxcd.io/HelmChart': ARTIFACT,
	'source.toolkit.fluxcd.io/Bucket': ARTIFACT,
	'cert-manager.io/Certificate': custom(['Expires', '.status.notAfter'], ['Renews', '.status.renewalTime']),
	'argoproj.io/Application': custom(['Revision', '.status.sync.revision'], ['Last sync', '.status.operationState.phase'])
};

/** The columns of a kind: its settings, else Kubri's own for it. */
export function columnsFor(key: string, settings: Record<string, ColumnSettings> | undefined): ColumnSettings {
	return settings?.[key] ?? defaultColumns(key);
}

/** Kubri's own columns for a kind, which resetting goes back to. */
export function defaultColumns(key: string): ColumnSettings {
	return BUILTIN[key] ?? NO_COLUMNS;
}

/** How the settings name a kind: `group/Kind`, `/Pod` for the core group. */
export function kindKey(resource: Pick<ResourceInfo, 'group' | 'kind'>) {
	return `${resource.group}/${resource.kind}`;
}

/** The custom columns' paths, one per line, for the table watch; null when there are none. */
export function pathsOf(settings: ColumnSettings): string | null {
	return settings.custom.length ? settings.custom.map((c) => c.path).join('\n') : null;
}

export interface Arrangeable {
	label: string;
	/** Only the wide view shows it, unless the settings say otherwise. */
	wideOnly: boolean;
}

/** Whether a column shows, by the settings and the wide view. */
export function isShown(column: Arrangeable, settings: ColumnSettings, wide: boolean) {
	if (settings.hidden.includes(column.label)) return false;
	return wide || !column.wideOnly || settings.shown.includes(column.label);
}

/**
 * The columns in the order the settings give. A column they do not name stays right after the
 * column it follows by nature, so new columns land where the server puts them.
 */
export function ordered<T extends Arrangeable>(columns: T[], settings: ColumnSettings): T[] {
	let last = -1;
	const keyed = columns.map((column, i) => {
		const at = settings.order.indexOf(column.label);
		if (at !== -1) last = at;
		return { column, key: at !== -1 ? at : last + (i + 1) / (columns.length + 1) };
	});
	return keyed.sort((a, b) => a.key - b.key).map((k) => k.column);
}

/** Settings with `label` moved by `delta` places among `labels`, the columns in their current order. */
export function moved(settings: ColumnSettings, labels: string[], label: string, delta: number): ColumnSettings {
	const order = [...labels];
	const from = order.indexOf(label);
	const to = from + delta;
	if (from === -1 || to < 0 || to >= order.length) return settings;
	order.splice(from, 1);
	order.splice(to, 0, label);
	return { ...settings, order };
}

/** Settings with `label` shown or hidden. */
export function toggled(settings: ColumnSettings, column: Arrangeable, show: boolean): ColumnSettings {
	const without = (list: string[]) => list.filter((l) => l !== column.label);
	return {
		...settings,
		hidden: show ? without(settings.hidden) : [...without(settings.hidden), column.label],
		shown: show && column.wideOnly ? [...without(settings.shown), column.label] : without(settings.shown)
	};
}

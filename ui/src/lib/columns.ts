import type { ColumnSettings, KindView, ResourceInfo } from './api';

export const NO_COLUMNS: ColumnSettings = { hidden: [], shown: [], order: [], custom: [] };

/** The columns of a kind: its settings, else its view's. */
export function columnsFor(
	key: string,
	settings: Record<string, ColumnSettings> | undefined,
	view: Pick<KindView, 'columns'> | undefined
): ColumnSettings {
	return settings?.[key] ?? defaultColumns(view);
}

/** The columns a kind's view gives it, which resetting goes back to. */
export function defaultColumns(view: Pick<KindView, 'columns'> | undefined): ColumnSettings {
	return view?.columns.length ? { ...NO_COLUMNS, custom: view.columns.map(({ name, path }) => ({ name, path })) } : NO_COLUMNS;
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

/** A column as the fitting sees it: how much it matters, and about how wide it is. */
export interface FitColumn {
	label: string;
	/** Higher stays longer; Infinity always stays. */
	rank: number;
	width: number;
}

/**
 * How much a column matters when the list is narrow: the name and the state always stay, then
 * readiness, the namespace and what the pods use; ages, restarts and the rest go first.
 */
export function columnRank(label: string, kind: 'namespace' | 'cell' | 'metric' | 'production', isName = false): number {
	if (isName) return Infinity;
	if (kind === 'namespace') return 85;
	if (kind === 'production') return 65;
	const name = label.toLowerCase();
	if (/^(status|phase|state|health)$/.test(name)) return Infinity;
	if (kind === 'metric') return name === 'cpu' || name === 'mem' ? 70 : 40;
	if (name === 'ready') return 90;
	if (/^(type|reason)$/.test(name)) return 80;
	if (/^(message|object)$/.test(name)) return 75;
	if (name === 'restarts') return 60;
	if (name === 'age' || name === 'last seen') return 55;
	return 50;
}

/** About how wide a column is, from its title and the longest of `texts`, as the list draws them. */
export function columnWidth(label: string, texts: string[], options: { mono?: boolean; icon?: boolean; max?: number } = {}): number {
	// Average glyph widths of the list's fonts: 13px text, 12.5px code, 12px titles with a sort arrow.
	const perChar = options.mono ? 7.6 : 7.1;
	const longest = texts.reduce((n, t) => Math.max(n, t.length), 0);
	const body = longest * perChar + (options.icon ? 21 : 0);
	const title = label.length * 6.9 + 14;
	return Math.min(Math.max(body, title), options.max ?? 420) + 16;
}

/** The labels of the columns to leave out so the rest fit in `room`, lowest rank and rightmost first. */
export function overflowing(columns: FitColumn[], room: number): string[] {
	let total = columns.reduce((n, c) => n + c.width, 0);
	const droppable = columns
		.map((c, at) => ({ ...c, at }))
		.filter((c) => Number.isFinite(c.rank))
		.sort((a, b) => a.rank - b.rank || b.at - a.at);
	const dropped = new Set<number>();
	for (const c of droppable) {
		if (total <= room) break;
		dropped.add(c.at);
		total -= c.width;
	}
	return columns.filter((_, at) => dropped.has(at)).map((c) => c.label);
}

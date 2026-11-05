/**
 * Where Kubri starts: the context used last, in the view it showed, unless the kubeconfig's
 * `current-context` changed since, which is the newer wish.
 */

/** What Kubri connects to when it starts. */
export type StartSetting = 'last' | 'kubeconfig';

/** The context used last, and the kubeconfig's `current-context` at that time. */
export interface LastContext {
	context: string;
	kubeconfig: string | null;
}

/** What a context showed, kept across restarts: no filter, scope or selection. */
export interface SavedView {
	page: string;
	resource: { group: string; version: string; plural: string } | null;
	namespace: string;
	xray: { group: string; version: string; plural: string } | null;
}

const START_KEY = 'kubri.start';
const LAST_KEY = 'kubri.lastContext';
const VIEWS_KEY = 'kubri.views';
/** Contexts whose view is kept. */
export const MAX_VIEWS = 10;

function read(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function write(key: string, value: string | null) {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		// Without storage, Kubri starts as the kubeconfig says.
	}
}

function parse<T>(text: string | null): T | null {
	if (!text) return null;
	try {
		return JSON.parse(text) as T;
	} catch {
		return null;
	}
}

export function startSetting(): StartSetting {
	return read(START_KEY) === 'kubeconfig' ? 'kubeconfig' : 'last';
}

export function setStartSetting(value: StartSetting) {
	write(START_KEY, value === 'last' ? null : value);
}

export function lastContext(): LastContext | null {
	const last = parse<LastContext>(read(LAST_KEY));
	return last && typeof last.context === 'string' ? { context: last.context, kubeconfig: last.kubeconfig ?? null } : null;
}

export function rememberContext(context: string, kubeconfig: string | null) {
	write(LAST_KEY, JSON.stringify({ context, kubeconfig } satisfies LastContext));
}

/**
 * The context to connect to on start: one the address names, else the one used last, else the
 * kubeconfig's `current-context`, else the first. The last one is skipped when it is gone, when
 * the setting says so, or when the kubeconfig's `current-context` changed since it was used.
 */
export function startContext(
	contexts: { name: string; isDefault: boolean }[],
	last: LastContext | null,
	setting: StartSetting,
	addressed: string | null = null
): string | null {
	const has = (name: string | null | undefined) => !!name && contexts.some((c) => c.name === name);
	if (has(addressed)) return addressed;
	const current = contexts.find((c) => c.isDefault)?.name ?? null;
	if (setting === 'last' && last && has(last.context) && last.kubeconfig === current) return last.context;
	return current ?? contexts[0]?.name ?? null;
}

function savedViews(): Record<string, SavedView & { at: number }> {
	const views = parse<Record<string, SavedView & { at: number }>>(read(VIEWS_KEY));
	return views && typeof views === 'object' && !Array.isArray(views) ? views : {};
}

/** Keeps the view of `context` for the next start, for the most recent contexts only. */
export function saveView(context: string, view: SavedView, now = Date.now()) {
	const views = savedViews();
	views[context] = { ...view, at: now };
	const kept = Object.entries(views)
		.sort(([, a], [, b]) => (b.at ?? 0) - (a.at ?? 0))
		.slice(0, MAX_VIEWS);
	write(VIEWS_KEY, JSON.stringify(Object.fromEntries(kept)));
}

/** The view `context` showed when Kubri last ran there. */
export function savedView(context: string): SavedView | null {
	const view = savedViews()[context];
	if (!view || typeof view.page !== 'string' || typeof view.namespace !== 'string') return null;
	return { page: view.page, resource: view.resource ?? null, namespace: view.namespace, xray: view.xray ?? null };
}

/** Forgets the views of contexts that are gone from the kubeconfig. */
export function keepViews(contexts: string[]) {
	const views = savedViews();
	const kept = Object.entries(views).filter(([name]) => contexts.includes(name));
	if (kept.length !== Object.keys(views).length) write(VIEWS_KEY, JSON.stringify(Object.fromEntries(kept)));
}

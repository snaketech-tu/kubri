/** What a context showed when you left it, so switching back lands in the same place. */
export interface Place<V> {
	/** The view on screen. */
	view: V;
	/** Back and forward history, and where in it the view is. */
	history: V[];
	historyAt: number;
	/** The resource list's filter. */
	filter: string;
	/** The object under the cursor, to put it there again. */
	cursor: { namespace: string | null; name: string } | null;
}

/** Remembers one place per context while Kubri runs. Nothing is saved to disk. */
export class Places<V> {
	#byContext = new Map<string, Place<V>>();

	save(context: string, place: Place<V>) {
		this.#byContext.set(context, place);
	}

	/** The place left in `context`, if any; it is forgotten, as it is about to be shown. */
	take(context: string): Place<V> | undefined {
		const place = this.#byContext.get(context);
		this.#byContext.delete(context);
		return place;
	}

	/** Forgets contexts that are gone from the kubeconfig. */
	keepOnly(contexts: string[]) {
		for (const name of [...this.#byContext.keys()]) {
			if (!contexts.includes(name)) this.#byContext.delete(name);
		}
	}
}

/**
 * The contexts in the switcher's order: as listed, except that with an empty search the
 * previous context comes first, so Enter goes back to it.
 */
export function switcherOrder<C extends { name: string }>(contexts: C[], previous: string | null, current: string | null, query: string): C[] {
	if (query || !previous || previous === current) return contexts;
	const at = contexts.findIndex((c) => c.name === previous);
	if (at <= 0) return contexts;
	return [contexts[at], ...contexts.slice(0, at), ...contexts.slice(at + 1)];
}

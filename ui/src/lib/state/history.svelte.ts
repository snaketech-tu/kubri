/** How many places the history keeps. */
const LIMIT = 50;

/** The places shown, for going back and forth like k9s `[`, `]` and `-`. */
export class History<V> {
	entries = $state.raw<V[]>([]);
	at = $state(-1);
	/** The place before the current one, for `-`. */
	#last: V | null = null;
	#same: (a: V, b: V) => boolean;

	constructor(same: (a: V, b: V) => boolean) {
		this.#same = same;
	}

	get canGoBack() {
		return this.at > 0;
	}

	get canGoForward() {
		return this.at < this.entries.length - 1;
	}

	/** Adds the place now shown after the current one, dropping those ahead of it. */
	push(view: V) {
		const current = this.entries[this.at];
		if (current && this.#same(current, view)) return;
		if (current) this.#last = current;
		this.entries = [...this.entries.slice(0, this.at + 1), view].slice(-LIMIT);
		this.at = this.entries.length - 1;
	}

	/** Moves back or forward and gives the place there; null when there is nowhere to go. */
	step(delta: -1 | 1): V | null {
		const view = this.entries[this.at + delta];
		if (!view) return null;
		this.#last = this.entries[this.at] ?? null;
		this.at += delta;
		return view;
	}

	/** Gives the place before this one and makes this one the place before; null without one. */
	toggleLast(): V | null {
		const last = this.#last;
		if (!last) return null;
		const at = this.entries.findLastIndex((v) => this.#same(v, last));
		this.#last = this.entries[this.at] ?? null;
		if (at !== -1) this.at = at;
		return last;
	}

	/** Puts back a history kept for later, or an empty one. */
	reset(entries: V[] = [], at = -1) {
		this.entries = entries;
		this.at = at;
		this.#last = null;
	}
}

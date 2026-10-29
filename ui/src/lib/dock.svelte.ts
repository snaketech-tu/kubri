import type { ObjectRef, ResourceInfo, TerminalMode } from './api';

/** A log or shell kept open in the workspace while you browse. */
export interface DockSession {
	id: number;
	kind: 'logs' | 'shell';
	resource: ResourceInfo;
	/** Carries the context it was opened in. */
	target: ObjectRef;
	/** The container to start with, or the default one. */
	container?: string;
	/** Logs of the instance that ended. */
	previous?: boolean;
	mode?: TerminalMode;
}

export type DockSide = 'bottom' | 'right';

const SIDE_KEY = 'kubri.dock.side';
const SIZE_KEY = 'kubri.dock.size';

function stored(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function store(key: string, value: string) {
	try {
		localStorage.setItem(key, value);
	} catch {
		// Kept in memory only.
	}
}

/** Same object and kind, so opening it again shows the open tab instead. */
function same(a: Omit<DockSession, 'id'>, b: Omit<DockSession, 'id'>) {
	return (
		a.kind === b.kind &&
		a.kind === 'logs' &&
		a.target.context === b.target.context &&
		a.target.namespace === b.target.namespace &&
		a.target.name === b.target.name &&
		a.resource.plural === b.resource.plural &&
		(a.container ?? '') === (b.container ?? '') &&
		!!a.previous === !!b.previous
	);
}

/**
 * The workspace: tabs of logs and shells that keep running while you browse, docked at
 * the bottom or right, or minimized into a task bar. Two of them can show side by side.
 */
class Dock {
	sessions = $state<DockSession[]>([]);
	/** The tab shown, on the left when split. */
	active = $state<number | null>(null);
	/** The tab shown beside the active one, or null for one at a time. */
	beside = $state<number | null>(null);
	/** The tab whose pane has the keyboard, and so the log keys. */
	focused = $state<number | null>(null);
	/** The tab last used, whose pane shows the next tab picked when split. */
	last = $state<number | null>(null);
	/** Only the task bar shows. */
	minimized = $state(false);
	#side = $state<DockSide>(stored(SIDE_KEY) === 'right' ? 'right' : 'bottom');
	#size = $state(Number(stored(SIZE_KEY)) || 0);
	#next = 0;

	get side() {
		return this.#side;
	}
	set side(value: DockSide) {
		this.#side = value;
		this.#size = 0;
		store(SIDE_KEY, value);
		store(SIZE_KEY, '0');
	}
	/** Pixels across the docked side, or 0 for the default. */
	get size() {
		return this.#size;
	}
	set size(value: number) {
		this.#size = Math.round(value);
		store(SIZE_KEY, String(this.#size));
	}

	get open() {
		return this.sessions.length > 0 && !this.minimized;
	}

	/** Opens a tab, or shows the one already open for the same logs. */
	add(session: Omit<DockSession, 'id'>) {
		const existing = this.sessions.find((s) => same(s, session));
		const id = existing?.id ?? ++this.#next;
		if (!existing) this.sessions.push({ ...session, id });
		this.show(id);
	}

	/** Shows a tab; when split, in the pane that last had the keyboard. */
	show(id: number) {
		this.minimized = false;
		if (id !== this.active && id !== this.beside) {
			if (this.beside !== null && this.last === this.beside) this.beside = id;
			else this.active = id;
		}
		this.last = id;
	}

	close(id: number) {
		const at = this.sessions.findIndex((s) => s.id === id);
		if (at < 0) return;
		this.sessions.splice(at, 1);
		if (this.beside === id) this.beside = null;
		if (this.active === id) {
			const next = this.beside ?? this.sessions[Math.min(at, this.sessions.length - 1)]?.id ?? null;
			this.active = next;
			if (next === this.beside) this.beside = null;
		}
		if (this.focused === id) this.focused = null;
		if (this.last === id) this.last = this.active;
	}

	/** Shows two tabs side by side, or one again. */
	toggleSplit() {
		if (this.beside !== null) {
			this.beside = null;
			return;
		}
		const other = [...this.sessions].reverse().find((s) => s.id !== this.active);
		if (other) this.beside = other.id;
	}

	/** Closes the tabs of other contexts, whose streams no longer work. */
	keepContext(context: string): number {
		const gone = this.sessions.filter((s) => s.target.context && s.target.context !== context);
		for (const s of gone) this.close(s.id);
		return gone.length;
	}
}

export const dock = new Dock();

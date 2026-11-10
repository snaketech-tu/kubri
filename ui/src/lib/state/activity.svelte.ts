import { errorMessage } from '../format';
import { t } from '../i18n';
import type { AppState } from '../state.svelte';
import type { Activity } from './model';

let activityId = 0;

/** Every change made from here in this session, newest first. */
export class Activities {
	entries = $state.raw<Activity[]>([]);
	/** Whether the activity panel is open. */
	open = $state(false);
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	/** Adds a change to the session activity; an entry with `undo` can be taken back. */
	record(entry: Omit<Activity, 'id' | 'at' | 'context' | 'undone'>): number {
		const id = ++activityId;
		this.entries = [{ ...entry, id, at: Date.now(), context: this.#app.cluster?.context ?? '', undone: false }, ...this.entries].slice(0, 200);
		return id;
	}

	/** Takes back an entry that can be, and records that it was. */
	async undo(id: number) {
		const entry = this.entries.find((a) => a.id === id);
		if (!entry?.undo || entry.undone) return;
		try {
			await entry.undo();
			this.entries = this.entries.map((a) => (a.id === id ? { ...a, undone: true } : a));
			this.#app.toast(t('Undone: {what}', { what: entry.text }));
		} catch (err) {
			this.#app.toast(errorMessage(err), 'bad');
		}
	}
}

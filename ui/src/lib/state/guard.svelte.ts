import { backend, type Guard } from '../api';
import { errorMessage } from '../format';
import { readOnlyReason } from '../guard';
import { t } from '../i18n';
import { drafts } from '../navigation.svelte';
import type { AppState } from '../state.svelte';

/** Whether the core lets Kubri change the connected cluster, which it enforces, and the lock that sets it. */
export class GuardRails {
	guard = $state<Guard>({ readOnly: false, reason: null, canUnlock: false, unlockedFor: null, fixed: false });
	/** When an unlocked context locks again, in milliseconds since the epoch. */
	unlockedUntil = $state<number | null>(null);
	#timer: ReturnType<typeof setTimeout> | null = null;
	#request = 0;
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	async refresh() {
		await this.#ask(() => backend.guard(), true);
	}

	/** Read-only until the core answers for a context just connected to; an older answer no longer counts. */
	checking() {
		this.#request++;
		this.#apply({ readOnly: true, reason: t('Kubri is still checking whether it may change this cluster'), canUnlock: false, unlockedFor: null, fixed: false });
		this.refresh();
	}

	/** Asks the core for the guard; only the latest answer counts, so a slow one cannot undo an unlock. */
	async #ask(ask: () => Promise<Guard>, quiet = false): Promise<boolean> {
		const request = ++this.#request;
		try {
			const guard = await ask();
			if (request === this.#request) this.#apply(guard);
			return true;
		} catch (err) {
			// Quietly keep the last state; the core still refuses what it must.
			if (!quiet) this.#app.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	#apply(guard: Guard) {
		this.guard = guard;
		this.unlockedUntil = guard.unlockedFor ? Date.now() + guard.unlockedFor * 1000 : null;
		if (this.#timer) clearTimeout(this.#timer);
		this.#timer = null;
		// Lock again on screen when the core does.
		if (guard.unlockedFor) this.#timer = setTimeout(() => this.refresh(), guard.unlockedFor * 1000 + 500);
	}

	/** Unlocks a locked context for 15 minutes. */
	async unlock() {
		if (drafts.block(() => this.unlock())) return;
		if (await this.#ask(() => backend.unlock())) this.#app.toast(t('Unlocked for 15 minutes'));
	}

	async lock() {
		if (drafts.block(() => this.lock())) return;
		await this.#ask(() => backend.lock());
	}

	/** Says why an action that changes the cluster did nothing, and how to allow it where that is possible. */
	explainReadOnly(action: string) {
		const app = this.#app;
		const what = action.replace(/…$/, '');
		app.toast(t('{action} is not available: {reason}', { action: what, reason: readOnlyReason(this.guard, app.contextSettings.readOnly, app.k9s?.readOnly ?? false) }), 'bad');
	}

	/** The header's lock button: read-only on and off, or a locked context unlocked and locked. */
	toggleReadOnly() {
		if (drafts.block(() => this.toggleReadOnly())) return;
		const context = this.#app.cluster?.context;
		if (!context) return;
		const settings = this.#app.contextSettings;
		if (settings.readOnly) {
			this.#app.prefs.setContextSettings(context, { ...settings, readOnly: false });
		} else if (this.guard.canUnlock) {
			if (this.guard.readOnly) this.unlock();
			else this.lock();
		} else {
			this.#app.prefs.setContextSettings(context, { ...settings, readOnly: true });
		}
	}
}

import { backend, type UpdateCheck } from '../api';
import { dismissRelease, dismissedRelease, releaseChecks, setReleaseChecks } from '../releases';
import type { AppState } from '../state.svelte';

/** New releases of Kubri, which it asks GitHub for when it starts. */
export class Releases {
	/** Whether Kubri asks for new releases when it starts. */
	checks = $state(releaseChecks());
	/** A release newer than this one, until the user waves it away. */
	latest = $state<UpdateCheck | null>(null);
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	setChecks(on: boolean) {
		this.checks = on;
		setReleaseChecks(on);
		if (on) this.check();
		else this.latest = null;
	}

	/** With `offer`, as when Kubri starts, opens the update dialog if Kubri can install the release. */
	async check(offer = false) {
		if (!this.checks) return;
		try {
			const latest = await backend.checkUpdate();
			this.latest = latest.newer && latest.version !== dismissedRelease() ? latest : null;
			if (offer && this.latest?.installable) this.#app.updateOpen = true;
		} catch {
			// Offline or rate-limited: no notice, and the next start asks again.
		}
	}

	/** Hides the release until the next one. */
	dismiss() {
		if (this.latest) dismissRelease(this.latest.version);
		this.latest = null;
		this.#app.updateOpen = false;
	}
}

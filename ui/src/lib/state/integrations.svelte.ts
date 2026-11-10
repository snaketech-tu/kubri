import { backend, type IntegrationId, type Integrations } from '../api';
import type { AppState } from '../state.svelte';

/** The tools the connected cluster runs that Kubri uses, like Prometheus or Flux. */
export class ClusterIntegrations {
	/** What was found, or null before the first answer for this context. */
	found = $state<Integrations | null>(null);
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	/** Finds the cluster's tools; `fresh` looks again and asks each whether it answers. */
	async load(fresh: boolean) {
		const context = this.#app.cluster?.context;
		try {
			const found = await backend.integrations(fresh);
			if (this.#app.cluster?.context === context) this.found = found;
		} catch {
			// Without them, Kubri works as before.
		}
	}

	/** Whether the cluster runs `id` and Kubri may use it. */
	has(id: IntegrationId): boolean {
		const item = this.found?.items.find((i) => i.id === id);
		return !!item && (item.source === 'found' || item.source === 'settings');
	}
}

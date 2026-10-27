import { backend, type ClusterInfo, type ContextInfo, type ResourceInfo } from './api';

export const ALL_NAMESPACES = '';

/** Global app state: kubeconfig contexts, the connected cluster and what is being browsed. */
class AppState {
	contexts = $state<ContextInfo[]>([]);
	cluster = $state<ClusterInfo | null>(null);
	resources = $state<ResourceInfo[]>([]);
	selected = $state<ResourceInfo | null>(null);
	namespace = $state(ALL_NAMESPACES);
	connecting = $state<string | null>(null);
	error = $state<string | null>(null);

	async load() {
		try {
			const { contexts, cluster } = await backend.contexts();
			this.contexts = contexts;
			if (cluster) {
				await this.connected(cluster);
			} else {
				const fallback = contexts.find((c) => c.isDefault) ?? contexts[0];
				if (fallback) await this.switchContext(fallback.name);
			}
		} catch (err) {
			this.error = message(err);
		}
	}

	async reloadContexts() {
		try {
			this.contexts = (await backend.reloadContexts()).contexts;
		} catch (err) {
			this.error = message(err);
		}
	}

	async switchContext(name: string) {
		this.connecting = name;
		this.error = null;
		try {
			await this.connected(await backend.switchContext(name));
		} catch (err) {
			this.error = `Could not connect to ${name}: ${message(err)}`;
		} finally {
			this.connecting = null;
		}
	}

	private async connected(cluster: ClusterInfo) {
		const resources = await backend.resources();
		const previous = this.selected;
		this.cluster = cluster;
		this.resources = resources;
		this.namespace = ALL_NAMESPACES;
		this.selected =
			resources.find((r) => previous && sameResource(r, previous)) ??
			resources.find((r) => r.group === '' && r.plural === 'pods') ??
			null;
	}
}

function message(err: unknown) {
	return err instanceof Error ? err.message : String(err);
}

export function sameResource(a: ResourceInfo, b: ResourceInfo) {
	return a.group === b.group && a.version === b.version && a.plural === b.plural;
}

export const app = new AppState();

import {
	backend,
	type ClusterInfo,
	type ContextInfo,
	type ObjectRef,
	type ResourceInfo,
	type ResourceRef
} from './api';
import { errorMessage } from './format';

export const ALL_NAMESPACES = '';

export type DetailsTab = 'describe' | 'yaml';

export interface Details {
	target: ObjectRef;
	resource: ResourceInfo;
	tab: DetailsTab;
	editing: boolean;
}

export type Dialog =
	| { kind: 'delete'; resource: ResourceInfo; targets: ObjectRef[] }
	| { kind: 'scale'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'restart'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'labels'; resource: ResourceInfo; target: ObjectRef };

export interface Toast {
	id: number;
	tone: 'ok' | 'bad';
	text: string;
}

/** Global app state: kubeconfig contexts, the connected cluster and what is being browsed. */
class AppState {
	contexts = $state<ContextInfo[]>([]);
	cluster = $state<ClusterInfo | null>(null);
	resources = $state<ResourceInfo[]>([]);
	namespaces = $state<string[]>([]);
	selected = $state<ResourceInfo | null>(null);
	namespace = $state(ALL_NAMESPACES);
	connecting = $state<string | null>(null);
	error = $state<string | null>(null);

	details = $state<Details | null>(null);
	dialog = $state<Dialog | null>(null);
	paletteOpen = $state(false);
	helpOpen = $state(false);
	contextMenuOpen = $state(false);
	toasts = $state<Toast[]>([]);
	#toastId = 0;

	/** True while an overlay owns the keyboard. */
	overlayOpen = $derived(this.paletteOpen || this.helpOpen || this.contextMenuOpen || this.dialog !== null);

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
			this.error = errorMessage(err);
		}
	}

	async reloadContexts() {
		try {
			this.contexts = (await backend.reloadContexts()).contexts;
		} catch (err) {
			this.error = errorMessage(err);
		}
	}

	async switchContext(name: string) {
		this.connecting = name;
		this.error = null;
		try {
			await this.connected(await backend.switchContext(name));
		} catch (err) {
			this.error = `Could not connect to ${name}: ${errorMessage(err)}`;
		} finally {
			this.connecting = null;
		}
	}

	/** Shows a resource list, optionally switching namespace, like k9s `:pods kube-system`. */
	open(resource: ResourceInfo, namespace?: string) {
		this.selected = resource;
		if (namespace !== undefined) this.namespace = namespace === 'all' ? ALL_NAMESPACES : namespace;
		this.details = null;
	}

	showDetails(resource: ResourceInfo, target: ObjectRef, tab: DetailsTab = 'describe', editing = false) {
		this.details = { resource, target, tab, editing };
	}

	toast(text: string, tone: Toast['tone'] = 'ok') {
		const id = ++this.#toastId;
		this.toasts.push({ id, tone, text });
		setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), tone === 'bad' ? 8000 : 4000);
	}

	private async connected(cluster: ClusterInfo) {
		const resources = await backend.resources();
		const previous = this.selected;
		this.cluster = cluster;
		this.resources = resources;
		this.namespace = ALL_NAMESPACES;
		this.details = null;
		this.selected =
			resources.find((r) => previous && sameResource(r, previous)) ??
			resources.find((r) => r.group === '' && r.plural === 'pods') ??
			null;
	}
}

export function sameResource(a: ResourceRef, b: ResourceRef) {
	return a.group === b.group && a.version === b.version && a.plural === b.plural;
}

export function refOf(resource: ResourceRef): ResourceRef {
	return { group: resource.group, version: resource.version, plural: resource.plural };
}

export const app = new AppState();

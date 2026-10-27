// Mirrors the serialized types of the kubri-core crate.

export interface ContextInfo {
	name: string;
	cluster: string | null;
	user: string | null;
	namespace: string | null;
	isDefault: boolean;
}

export interface ClusterInfo {
	context: string;
	server: string;
	version: string;
	defaultNamespace: string;
}

export interface ContextList {
	contexts: ContextInfo[];
	cluster: ClusterInfo | null;
}

export interface ResourceRef {
	/** Empty for the core group. */
	group: string;
	version: string;
	plural: string;
}

export interface ResourceInfo extends ResourceRef {
	kind: string;
	namespaced: boolean;
	verbs: string[];
}

export interface KubeObject {
	apiVersion?: string;
	kind?: string;
	metadata: {
		name: string;
		namespace?: string;
		uid?: string;
		creationTimestamp?: string;
		labels?: Record<string, string>;
		[key: string]: unknown;
	};
	[key: string]: unknown;
}

export type WatchEvent =
	| { type: 'reset'; items: KubeObject[] }
	| { type: 'upsert'; item: KubeObject }
	| { type: 'delete'; item: KubeObject }
	| { type: 'error'; message: string };

/** The operations both the desktop app and kubri-server offer to the UI. */
export interface Backend {
	contexts(): Promise<ContextList>;
	reloadContexts(): Promise<ContextList>;
	switchContext(name: string): Promise<ClusterInfo>;
	resources(): Promise<ResourceInfo[]>;
	/** Starts a watch and returns a function that stops it. */
	watch(resource: ResourceRef, namespace: string | null, onEvent: (event: WatchEvent) => void): () => void;
}

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
	singular: string;
	/** Aliases such as `po` or `deploy`. */
	shortNames: string[];
	namespaced: boolean;
	verbs: string[];
	/** Subresources such as `scale`, `log` or `exec`. */
	subresources: string[];
}

export interface Column {
	name: string;
	type: string;
	format: string;
	description: string;
	/** 0 for the default view, higher for the wide view. */
	priority: number;
}

export interface Row {
	key: string;
	name: string;
	namespace: string | null;
	created: string | null;
	labels: Record<string, string>;
	cells: unknown[];
}

export type TableEvent =
	| { type: 'reset'; columns: Column[]; rows: Row[] }
	| { type: 'upsert'; row: Row }
	| { type: 'delete'; row: Row }
	| { type: 'error'; message: string };

export interface ObjectRef {
	resource: ResourceRef;
	namespace: string | null;
	name: string;
}

export interface KubeObject {
	apiVersion?: string;
	kind?: string;
	metadata: {
		name: string;
		namespace?: string;
		uid?: string;
		resourceVersion?: string;
		creationTimestamp?: string;
		deletionTimestamp?: string;
		labels?: Record<string, string>;
		annotations?: Record<string, string>;
		ownerReferences?: { kind: string; name: string; controller?: boolean }[];
		managedFields?: unknown[];
		[key: string]: unknown;
	};
	spec?: Record<string, any>;
	status?: Record<string, any>;
	[key: string]: unknown;
}

export interface DeleteOptions {
	gracePeriodSeconds?: number | null;
	force?: boolean;
	propagation?: 'background' | 'foreground' | 'orphan';
}

export interface EventInfo {
	kind: string;
	reason: string;
	message: string;
	count: number;
	source: string;
	firstSeen: string | null;
	lastSeen: string | null;
}

export type MetricsKind = 'pods' | 'nodes';

export interface Usage {
	namespace: string | null;
	name: string;
	/** Millicores. */
	cpu: number;
	/** Bytes. */
	memory: number;
}

/** The operations both the desktop app and kubri-server offer to the UI. */
export interface Backend {
	contexts(): Promise<ContextList>;
	reloadContexts(): Promise<ContextList>;
	switchContext(name: string): Promise<ClusterInfo>;
	resources(): Promise<ResourceInfo[]>;
	/** Starts a live table and returns a function that stops it. */
	watchTable(resource: ResourceRef, namespace: string | null, onEvent: (event: TableEvent) => void): () => void;
	getObject(target: ObjectRef): Promise<KubeObject>;
	replaceObject(target: ObjectRef, value: KubeObject): Promise<KubeObject>;
	/** JSON merge patch. */
	patchObject(target: ObjectRef, patch: unknown): Promise<KubeObject>;
	deleteObject(target: ObjectRef, options?: DeleteOptions): Promise<void>;
	scale(target: ObjectRef, replicas: number): Promise<void>;
	restart(target: ObjectRef): Promise<void>;
	events(namespace: string | null, uid: string): Promise<EventInfo[]>;
	metrics(kind: MetricsKind, namespace: string | null): Promise<Usage[]>;
}

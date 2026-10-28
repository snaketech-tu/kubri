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

export interface LogOptions {
	/** Only this container; all containers when unset. */
	container?: string | null;
	follow?: boolean;
	previous?: boolean;
	timestamps?: boolean;
	tailLines?: number | null;
	sinceSeconds?: number | null;
}

export interface LogSource {
	pod: string;
	container: string;
}

export interface LogLine {
	/** Index into the sources of the stream. */
	source: number;
	text: string;
}

export type LogEvent =
	| { type: 'sources'; sources: LogSource[] }
	| { type: 'lines'; lines: LogLine[] }
	| { type: 'error'; source: number | null; message: string }
	| { type: 'end' };

export type TerminalMode = 'shell' | 'attach';

export interface TerminalOptions {
	container?: string | null;
	mode?: TerminalMode;
	command?: string[] | null;
	cols: number;
	rows: number;
}

export type TerminalEvent =
	| { type: 'ready'; container: string }
	| { type: 'data'; data: Uint8Array }
	| { type: 'exit'; message: string | null };

export interface TerminalSession {
	write(data: string): void;
	resize(cols: number, rows: number): void;
	close(): void;
}

export interface PortForwardRequest {
	target: ObjectRef;
	/** The container port, or the Service port for Services. */
	remotePort: number;
	/** Empty picks a free port. */
	localPort?: number | null;
	address?: string | null;
}

export interface PortForwardInfo {
	id: number;
	context: string;
	target: ObjectRef;
	namespace: string;
	pod: string;
	remotePort: number;
	podPort: number;
	address: string;
	localPort: number;
	started: string;
	connections: number;
	error: string | null;
}

export interface DrainOptions {
	ignoreDaemonsets?: boolean;
	deleteEmptydirData?: boolean;
	force?: boolean;
	gracePeriodSeconds?: number | null;
}

export interface PodIssue {
	pod: string;
	reason: string;
}

export interface DrainReport {
	evicted: string[];
	skipped: PodIssue[];
	failed: PodIssue[];
}

export type Health = 'ok' | 'warning' | 'failed';

/** How many objects of one kind are healthy. */
export interface KindPulse {
	kind: string;
	resource: ResourceRef;
	total: number;
	ok: number;
	warning: number;
	failed: number;
	/** Why the kind could not be listed, for example missing permissions. */
	error: string | null;
}

export interface RecentEvent extends EventInfo {
	objectKind: string;
	object: ObjectRef;
}

export interface Utilization {
	/** Measured by metrics-server; null when it is not installed. */
	used: number | null;
	/** Requested by the pods that are not finished. */
	requested: number | null;
	/** Allocatable on all nodes. */
	capacity: number | null;
}

export interface Pulse {
	kinds: KindPulse[];
	/** Events of the last hour. */
	events: { normal: number; warning: number; recent: RecentEvent[]; error: string | null };
	/** In millicores. */
	cpu: Utilization;
	/** In bytes. */
	memory: Utilization;
}

export interface XrayNode {
	/** Unique in the tree: the path from the root. */
	id: string;
	kind: string;
	name: string;
	namespace: string | null;
	/** Set for objects that can be opened. */
	resource: ResourceRef | null;
	health: Health;
	detail: string;
	children: XrayNode[];
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
	/** Streams logs of a pod, workload or Service and returns a function that stops it. */
	streamLogs(target: ObjectRef, options: LogOptions, onEvent: (event: LogEvent) => void): () => void;
	/** Opens a shell or attaches to a container. */
	openTerminal(target: ObjectRef, options: TerminalOptions, onEvent: (event: TerminalEvent) => void): TerminalSession;
	portForwards(): Promise<PortForwardInfo[]>;
	startPortForward(request: PortForwardRequest): Promise<PortForwardInfo>;
	stopPortForward(id: number): Promise<void>;
	cordon(node: string, cordoned: boolean): Promise<void>;
	drain(node: string, options: DrainOptions): Promise<DrainReport>;
	/** The cluster overview; null covers all namespaces. */
	pulse(namespace: string | null): Promise<Pulse>;
	/** The XRay trees of every object of `resource`. */
	xray(resource: ResourceRef, namespace: string | null): Promise<XrayNode[]>;
}

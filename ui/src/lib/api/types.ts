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
	/** The objects this one belongs to, such as the ReplicaSet of a pod. */
	owners: Owner[];
	cells: unknown[];
}

export interface Owner {
	apiVersion: string;
	kind: string;
	name: string;
	uid: string;
	/** The owner that manages the object. */
	controller: boolean;
}

/** Narrows a list on the server, like `kubectl get -l` and `--field-selector`. */
export interface TableScope {
	labelSelector?: string | null;
	fieldSelector?: string | null;
}

/** Where Enter leads from an object: a list of what belongs to it. */
/** An object that uses a ConfigMap, Secret, PVC, ServiceAccount or PriorityClass. */
export interface Reference {
	resource: ResourceRef;
	kind: string;
	namespace: string | null;
	name: string;
}

export interface Drill {
	resource: ResourceRef;
	/** Null lists all namespaces. */
	namespace: string | null;
	scope: TableScope;
	/** Keeps only what this UID owns, like the Jobs of a CronJob. */
	ownerUid: string | null;
	/** Such as `Pods of Deployment web`. */
	title: string;
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
		ownerReferences?: { apiVersion?: string; kind: string; name: string; uid?: string; controller?: boolean }[];
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
	/** What a pod's containers request and may use at most; a limit only when all set one. */
	requests?: Limits;
	limits?: Limits;
	/** What a node can give to pods. */
	allocatable?: Limits;
}

/** Millicores and bytes, where set. */
export interface Limits {
	cpu?: number;
	memory?: number;
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

export interface BenchRequest {
	forwardId: number;
	method: string;
	path: string;
	/** `Host` replaces the default `localhost:<port>`. */
	headers: Record<string, string>;
	body: string | null;
	concurrency: number;
	/** Either a number of requests or a duration. */
	requests: number | null;
	durationSecs: number | null;
	timeoutSecs: number | null;
}

export type BenchState = 'running' | 'done' | 'stopped';

export interface BenchResults {
	total: number;
	/** Seconds, like every time here. */
	duration: number;
	requestsPerSecond: number;
	fastest: number | null;
	slowest: number | null;
	average: number | null;
	percentiles: { percent: number; seconds: number }[];
	histogram: { from: number; to: number; count: number }[];
	statusCodes: { code: number; count: number }[];
	errors: { message: string; count: number }[];
	bytes: number;
}

export interface BenchInfo {
	id: number;
	forwardId: number;
	target: ObjectRef;
	pod: string;
	url: string;
	method: string;
	concurrency: number;
	requests: number | null;
	durationSecs: number | null;
	/** What was asked for, so the run can be repeated. */
	path: string;
	headers: Record<string, string>;
	body: string | null;
	timeoutSecs: number;
	started: string;
	state: BenchState;
	completed: number;
	elapsed: number;
	results: BenchResults | null;
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

/** One rule of a Role or ClusterRole. */
export interface Rule {
	apiGroups: string[];
	resources: string[];
	resourceNames: string[];
	nonResourceUrls: string[];
	verbs: string[];
}

/** `missing`: the binding points to a role that does not exist. `unreadable`: roles could not be listed. */
export type RoleState = 'found' | 'missing' | 'unreadable';

/** What one binding grants one subject. */
export interface Grant {
	bindingKind: 'RoleBinding' | 'ClusterRoleBinding';
	binding: string;
	roleKind: 'Role' | 'ClusterRole';
	role: string;
	/** Where the rules apply; null for the whole cluster. */
	namespace: string | null;
	roleState: RoleState;
	rules: Rule[];
}

export interface SubjectInfo {
	kind: 'User' | 'Group' | 'ServiceAccount';
	name: string;
	namespace: string | null;
	grants: Grant[];
}

export interface Rbac {
	subjects: SubjectInfo[];
	/** Lists that failed, for example because of missing permissions. */
	errors: string[];
}

/** What the current user may do, like `kubectl auth can-i --list`. */
export interface Permissions {
	namespace: string;
	rules: Rule[];
	incomplete: boolean;
	error: string | null;
}

export type Level = 'info' | 'warning' | 'error';

export interface Issue {
	level: Level;
	/** A stable name for the check, such as `image-tag`. */
	code: string;
	message: string;
	container: string | null;
}

export interface Finding {
	namespace: string | null;
	name: string;
	level: Level;
	issues: Issue[];
}

export interface SanitizerSection {
	kind: string;
	resource: ResourceRef;
	total: number;
	/** Objects without warnings or errors. */
	ok: number;
	warning: number;
	error: number;
	/** 0 to 100; null when the kind could not be listed. */
	score: number | null;
	findings: Finding[];
	failed: string | null;
}

export interface SanitizerReport {
	score: number;
	grade: string;
	namespace: string | null;
	sections: SanitizerSection[];
}

export interface HelmRelease {
	name: string;
	namespace: string;
	revision: number;
	status: string;
	chart: string;
	chartVersion: string;
	appVersion: string;
	updated: string | null;
	description: string;
}

export interface HelmReleaseDetails extends HelmRelease {
	notes: string;
	/** The values given at install or upgrade. */
	values: unknown;
	/** Chart defaults merged with the given values. */
	allValues: unknown;
	manifest: string;
}

export interface ConfigFile {
	path: string;
	error: string | null;
}

export interface Alias {
	name: string;
	/** What it stands for, such as `v1/pods` or `pod kube-system app=dns`. */
	command: string;
}

export interface Hotkey {
	name: string;
	shortCut: string;
	description: string;
	/** A command as typed after `:`, which may use the plugin variables. */
	command: string;
	keepHistory: boolean;
	/** Takes the key even where Kubri uses it. */
	override: boolean;
}

export interface Plugin {
	name: string;
	shortCut: string;
	description: string;
	/** Resource names the plugin is offered for, or `all`. */
	scopes: string[];
	command: string;
	args: string[];
	/** Runs without a terminal, and Kubri does not wait for it. */
	background: boolean;
	/** Shows the command and asks before running it. */
	confirm: boolean;
	dangerous: boolean;
	override: boolean;
	/** Values asked for before it runs, as `$INPUT_<NAME>`. */
	inputs: PluginInput[];
}

export interface PluginInput {
	name: string;
	label: string;
	type: 'string' | 'number' | 'bool' | 'dropdown';
	required: boolean;
	default: string;
	/** The choices of a dropdown. */
	options: string[];
}

/** Percentages at which %CPU and %MEM show as a warning and as critical. */
export interface Threshold {
	warn: number;
	critical: number;
}

export interface LoggerSettings {
	/** Lines to start with. */
	tail: number;
	/** How far back to start, in seconds; -1 for the tail only. */
	sinceSeconds: number;
	textWrap: boolean;
	showTime: boolean;
	disableAutoscroll: boolean;
}

/** A rule of `jumps.yaml`: where Enter leads from a resource. */
export interface Jump {
	source: string;
	targetGVR: string;
	labelSelector: string;
	fieldSelector: string;
	targetNamespace: string;
}

/** The k9s aliases, hotkeys, plugins and skins for the connected context. */
export interface K9sConfig {
	/** Where the files were looked for. */
	dirs: string[];
	files: ConfigFile[];
	aliases: Alias[];
	hotkeys: Hotkey[];
	plugins: Plugin[];
	/** Skins in the skins directories, by name. */
	skins: string[];
	/** The skin k9s uses for the context. */
	skin: string | null;
	/** Nothing in the cluster may be changed: k9s `readOnly`, or Kubri started read-only. */
	readOnly: boolean;
	/** The command to open after connecting, such as `pods`. */
	defaultView: string | null;
	thresholds: { cpu: Threshold; memory: Threshold };
	logger: LoggerSettings;
	/** Where port-forwards listen by default. */
	portForwardAddress: string | null;
	jumps: Jump[];
}

export interface Cell {
	name: string;
	value: string;
}

/** A plugin by name, for the selected object, row or nothing. */
export interface PluginRun {
	plugin: string;
	target?: ObjectRef | null;
	/** The view's resource, when nothing is selected. */
	resource?: ResourceRef | null;
	/** The selected Helm release, in the Helm view. */
	release?: { namespace: string; name: string } | null;
	container?: string | null;
	/** The list filter, as `$FILTER`. */
	filter?: string | null;
	/** The selected row's cells, for `$COL-<NAME>`. */
	columns?: Cell[];
	/** Values for the plugin's inputs, by name. */
	inputs?: Record<string, string>;
	cols?: number;
	rows?: number;
}

/** The operations both the desktop app and kubri-server offer to the UI. */
export interface Backend {
	contexts(): Promise<ContextList>;
	reloadContexts(): Promise<ContextList>;
	switchContext(name: string): Promise<ClusterInfo>;
	resources(): Promise<ResourceInfo[]>;
	/** Starts a live table and returns a function that stops it. */
	watchTable(
		resource: ResourceRef,
		namespace: string | null,
		scope: TableScope | null,
		onEvent: (event: TableEvent) => void
	): () => void;
	getObject(target: ObjectRef): Promise<KubeObject>;
	replaceObject(target: ObjectRef, value: KubeObject): Promise<KubeObject>;
	/** JSON merge patch. */
	patchObject(target: ObjectRef, patch: unknown): Promise<KubeObject>;
	deleteObject(target: ObjectRef, options?: DeleteOptions): Promise<void>;
	scale(target: ObjectRef, replicas: number): Promise<void>;
	restart(target: ObjectRef): Promise<void>;
	/** Where Enter leads from an object; null when it has nowhere to go but its details. */
	drill(target: ObjectRef): Promise<Drill | null>;
	/** What uses a ConfigMap, Secret, PVC, ServiceAccount or PriorityClass, like k9s `u`. */
	usedBy(target: ObjectRef): Promise<Reference[]>;
	/** Runs a CronJob now; returns the Job's name. */
	triggerCronJob(target: ObjectRef): Promise<string>;
	suspendCronJob(target: ObjectRef, suspend: boolean): Promise<void>;
	setImages(target: ObjectRef, images: ContainerImage[]): Promise<void>;
	/** Rolls a ReplicaSet's Deployment back to it; returns the Deployment's name. */
	rollback(target: ObjectRef): Promise<string>;
	events(namespace: string | null, uid: string): Promise<EventInfo[]>;
	metrics(kind: MetricsKind, namespace: string | null): Promise<Usage[]>;
	/** Streams logs of a pod, workload or Service and returns a function that stops it. */
	streamLogs(target: ObjectRef, options: LogOptions, onEvent: (event: LogEvent) => void): () => void;
	/** Opens a shell or attaches to a container. */
	openTerminal(target: ObjectRef, options: TerminalOptions, onEvent: (event: TerminalEvent) => void): TerminalSession;
	portForwards(): Promise<PortForwardInfo[]>;
	startPortForward(request: PortForwardRequest): Promise<PortForwardInfo>;
	stopPortForward(id: number): Promise<void>;
	/** Newest first, with live progress while running. */
	benchmarks(): Promise<BenchInfo[]>;
	startBenchmark(request: BenchRequest): Promise<BenchInfo>;
	stopBenchmark(id: number): Promise<void>;
	removeBenchmark(id: number): Promise<void>;
	cordon(node: string, cordoned: boolean): Promise<void>;
	drain(node: string, options: DrainOptions): Promise<DrainReport>;
	/** The cluster overview; null covers all namespaces. */
	pulse(namespace: string | null): Promise<Pulse>;
	/** The XRay trees of every object of `resource`. */
	xray(resource: ResourceRef, namespace: string | null): Promise<XrayNode[]>;
	/** Checks the objects of a namespace, or of the whole cluster, like Popeye. */
	sanitize(namespace: string | null): Promise<SanitizerReport>;
	rbac(namespace: string | null): Promise<Rbac>;
	/** The current user's rules; null uses the context's namespace. */
	permissions(namespace: string | null): Promise<Permissions>;
	helmReleases(namespace: string | null): Promise<HelmRelease[]>;
	/** The latest revision unless `revision` is given. */
	helmRelease(namespace: string, name: string, revision?: number): Promise<HelmReleaseDetails>;
	helmHistory(namespace: string, name: string): Promise<HelmRelease[]>;
	/** The installed helm command's version; rollback and uninstall need it. */
	helmVersion(): Promise<string | null>;
	helmRollback(namespace: string, name: string, revision: number): Promise<string>;
	helmUninstall(namespace: string, name: string): Promise<string>;
	k9sConfig(): Promise<K9sConfig>;
	/** A skin file as JSON. */
	k9sSkin(name: string): Promise<unknown>;
	/** Starts a background plugin. */
	runPlugin(run: PluginRun): Promise<void>;
	/** Runs a plugin in a terminal. */
	openPlugin(run: PluginRun, onEvent: (event: TerminalEvent) => void): TerminalSession;
	/** The desktop app, where port-forwards and plugins run on this computer. */
	desktop: boolean;
	/** Opens a web address in a new tab, or in the default browser from the desktop app. */
	openUrl(url: string): Promise<void>;
	/** Saves text as a file. Returns the path when the desktop app saved it to Downloads. */
	saveText(name: string, text: string): Promise<string | null>;
}

export interface ContainerImage {
	name: string;
	image: string;
	/** An init container. */
	init?: boolean;
}

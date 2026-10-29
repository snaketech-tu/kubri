// Mirrors the serialized types of the kubri-core crate.

import type { KindSchema } from '../schema';

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

/** What Kubri remembers about a context, in its own settings file. */
export interface ContextSettings {
	/** Shown instead of the context's name when set. */
	displayName: string;
	/** Such as `Staging`, shown beside the name. */
	label: string;
	/** `#rrggbb`, or empty. */
	color: string;
	/** Destructive actions ask to type the name; a banner says where you are. */
	production: boolean;
	readOnly: boolean;
	/** Starts read-only; unlocking lasts 15 minutes. */
	locked: boolean;
	/** The namespace shown after connecting; empty for all. */
	namespace: string;
	/** AI agents may change this context, each change confirmed in Kubri. Never on production. */
	agentWrites: boolean;
}

/** Which actions show a dry-run diff first. YAML edits always do. */
export interface DiffSettings {
	scale: boolean;
	restart: boolean;
	image: boolean;
	labels: boolean;
}

/** What AI agents connected over MCP may do. */
export interface AgentSettings {
	/** Serve MCP over HTTP on localhost from the running app. */
	http: boolean;
	/** The desktop app's port for it. */
	port: number;
	/** Agents may read Secret values. */
	secrets: boolean;
}

/** Where Explain sends its questions; `none` sends nothing anywhere. */
export type ExplainProvider = 'none' | 'anthropic' | 'openai';

/** The model Explain asks. The API key is kept apart, never in the settings. */
export interface ExplainSettings {
	provider: ExplainProvider;
	/** Empty for the provider's own address. */
	baseUrl: string;
	/** Empty for the default model, where the provider has one. */
	model: string;
}

export interface Settings {
	contexts: Record<string, ContextSettings>;
	diff: DiffSettings;
	agents: AgentSettings;
	explain: ExplainSettings;
	debug: DebugSettings;
}

export interface DebugSettings {
	/** The image last used for a debug container. */
	image: string;
}

/** A debug container to start, like `kubectl debug`. */
export type DebugRequest =
	/** An ephemeral container in the pod that sees the processes of `container`. */
	| { kind: 'ephemeral'; target: ObjectRef; container: string | null; image: string | null }
	/** A copy of the pod in which `container` sleeps instead of running its command. */
	| { kind: 'copy'; target: ObjectRef; container: string | null }
	/** A privileged pod on the node with a shell in the node's namespaces. */
	| { kind: 'node'; node: string; namespace: string | null; image: string | null };

/** A higher memory limit for a container that ran out of memory, on the workload that owns the pod. */
export interface MemoryFix {
	/** The Deployment, StatefulSet or DaemonSet to change; null for a pod nothing owns. */
	workload: ObjectRef | null;
	container: string;
	limit: string | null;
	request: string | null;
	suggested: string;
}

/** Why a node would refuse a pending pod; no reasons where Kubri finds none. */
export interface NodeFit {
	node: string;
	reasons: string[];
}

/** Which pods a Service picks, and which nearly match. */
export interface ServiceCheck {
	selector: Record<string, string>;
	pods: { name: string; ready: boolean; missingPorts: string[] }[];
	near: { name: string; differences: string[] }[];
}

/** Objects to create or change from YAML, as `kubectl apply --server-side` does. */
export interface ApplyRequest {
	objects: unknown[];
	/** For namespaced objects that name none. */
	namespace?: string | null;
	/** Only a server-side dry run: nothing changes. */
	dryRun?: boolean;
	/** Refuse to apply if the connected context is no longer this one. */
	context?: string | null;
	/** Apply in this context instead of the connected one. */
	into?: string | null;
}

export type ApplyOutcome = 'created' | 'changed' | 'unchanged' | 'unchecked' | 'failed';

/** What happened, or would happen, to one object. */
export interface ApplyItem {
	/** Its place in the request. */
	index: number;
	kind: string;
	name: string;
	namespace: string | null;
	target: ObjectRef | null;
	outcome: ApplyOutcome;
	before: KubeObject | null;
	after: KubeObject | null;
	message: string | null;
}

/** Where to connect once the debug container runs. */
export interface DebugSession {
	pod: ObjectRef;
	container: string;
	mode: TerminalMode;
	/** What is left behind, such as a copy to delete when done. */
	note?: string;
}

/** What to explain. */
export type ExplainSubject =
	| { kind: 'object'; target: ObjectRef }
	| { kind: 'logs'; target: ObjectRef; container: string | null; lines: string }
	| { kind: 'finding'; target: ObjectRef; issues: string[] }
	| { kind: 'failure'; action: string; target: ObjectRef | null; error: string };

/** Exactly what Explain would send, to look at first. */
export interface ExplainPrompt {
	title: string;
	system: string;
	text: string;
	provider: ExplainProvider;
	model: string | null;
	endpoint: string | null;
	hasKey: boolean;
}

export interface ExplainAnswer {
	/** Markdown. */
	text: string;
	model: string;
	/** The answer ended at the length limit. */
	cutShort: boolean;
}

/** How to connect an agent. */
export interface AgentConnection {
	/** The HTTP endpoint while it runs. */
	url: string | null;
	/** Why it does not run although it is on. */
	error: string | null;
	token: string | null;
	/** The program and arguments for MCP over stdio. */
	command: string[];
}

/** A change an agent asked for, waiting for the user. */
export interface AgentRequest {
	id: number;
	tool: string;
	context: string | null;
	summary: string;
	kubectl: string;
}

/** One tool call of an agent. */
export interface AgentCall {
	/** Seconds since the epoch. */
	time: number;
	transport: 'stdio' | 'http';
	tool: string;
	context: string | null;
	arguments: Record<string, unknown>;
	error: string | null;
	millis: number;
}

export interface SettingsReply {
	settings: Settings;
	/** Why the settings file could not be read. */
	error: string | null;
}

/** A change to try as a server-side dry run. */
export type Change =
	| { type: 'replace'; value: KubeObject }
	| { type: 'patch'; patch: unknown }
	| { type: 'scale'; replicas: number }
	| { type: 'restart' }
	| { type: 'images'; images: ContainerImage[] }
	/** One container's memory limit in a workload's pod template. */
	| { type: 'memory'; container: string; memory: string };

/** An object now and after a change, without status and fields that change on every write. */
export interface Preview {
	before: unknown;
	after: unknown;
}

/** One action as RBAC sees it, like `kubectl auth can-i`. */
export interface AccessCheck {
	verb: string;
	/** Empty for the core group. */
	group: string;
	/** Plural, such as `pods`. */
	resource: string;
	/** Such as `exec` or `scale`. */
	subresource?: string;
	/** Null for cluster-scoped resources or all namespaces. */
	namespace?: string | null;
	name?: string | null;
}

/** Who the cluster takes the user for, like `kubectl auth whoami`. */
export interface Identity {
	username: string;
	groups: string[];
}

/** Whether Kubri may change the connected cluster. */
export interface Guard {
	readOnly: boolean;
	reason: string | null;
	/** Locked, and nothing else keeps it read-only. */
	canUnlock: boolean;
	/** Seconds until an unlocked context locks again. */
	unlockedFor: number | null;
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
	| { type: 'error'; message: string }
	/** Something to know about the list, such as which namespaces it covers. */
	| { type: 'notice'; message: string };

export interface ObjectRef {
	context?: string;
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

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogLine {
	/** Index into the sources of the stream. */
	source: number;
	/** The line without the kubelet's timestamp. */
	text: string;
	/** When the kubelet received the line, as RFC 3339. */
	time?: string;
	/** The level the line names; lines that continue another take its level. */
	level?: LogLevel;
	/** The fields of a JSON or logfmt line. */
	fields?: Record<string, unknown>;
	/** The line continues the one before it from the same container, like a stack frame. */
	continued?: boolean;
}

export type LogEvent =
	| { type: 'sources'; sources: LogSource[] }
	| { type: 'lines'; lines: LogLine[] }
	| { type: 'error'; source: number | null; message: string }
	/** Something happened to the stream, such as a pod that joined or a log that ended. */
	| { type: 'marker'; text: string; time?: string }
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

/** A chart by name in a repository URL, as `repo/chart` from a repository added to helm, or `oci://…`. */
export interface HelmChartRef {
	chart: string;
	repo?: string | null;
	/** The latest when empty. */
	version?: string | null;
}

/** An install or upgrade; see `Backend.helmChange`. */
export interface HelmChange {
	namespace: string;
	name: string;
	/** For an upgrade, none keeps the chart and version the release runs. */
	chart?: HelmChartRef | null;
	/** Values as YAML; they replace the values given before. */
	values: string;
	install?: boolean;
	createNamespace?: boolean;
	dryRun?: boolean;
}

export interface HelmOutcome extends HelmRelease {
	notes: string;
	manifest: string;
	/** Hooks the chart runs around the change; they are not in the manifest. */
	hooks: number;
}

/** A chart found on Artifact Hub. */
export interface HubChart {
	name: string;
	version: string;
	appVersion: string;
	description: string;
	url: string;
	repoName: string;
	repoUrl: string;
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
	/** Whether each check is allowed; null where the API server cannot say. */
	access(checks: AccessCheck[], fresh?: boolean): Promise<(boolean | null)[]>;
	whoAmI(): Promise<Identity | null>;
	/** All namespaces, or those the kubeconfig and settings name where listing them is not allowed. */
	readableNamespaces(): Promise<string[]>;
	settings(): Promise<SettingsReply>;
	setContextSettings(name: string, settings: ContextSettings): Promise<Settings>;
	setDiffSettings(settings: DiffSettings): Promise<Settings>;
	setAgentSettings(settings: AgentSettings): Promise<Settings>;
	agentConnection(): Promise<AgentConnection>;
	/** Makes a new token; agents with the old one are refused. */
	newAgentToken(): Promise<AgentConnection>;
	/** Changes agents wait to have allowed; asking marks the window as open. */
	agentRequests(): Promise<AgentRequest[]>;
	answerAgent(id: number, allow: boolean): Promise<boolean>;
	agentCalls(): Promise<AgentCall[]>;
	setExplainSettings(settings: ExplainSettings): Promise<Settings>;
	/** Whether an API key for Explain is stored; the key itself never comes back. */
	explainKey(): Promise<{ hasKey: boolean }>;
	/** Stores the key, or removes it with null. */
	setExplainKey(key: string | null): Promise<{ hasKey: boolean }>;
	/** Gathers what Explain would send; nothing leaves Kubri yet. */
	explainPrompt(subject: ExplainSubject): Promise<ExplainPrompt>;
	/** Sends the prompt's text to the model in the settings. */
	explain(text: string): Promise<ExplainAnswer>;
	guard(): Promise<Guard>;
	/** Unlocks a locked context for 15 minutes. */
	unlock(): Promise<Guard>;
	lock(): Promise<Guard>;
	/** Starts a live table and returns a function that stops it. */
	watchTable(
		resource: ResourceRef,
		namespace: string | null,
		scope: TableScope | null,
		onEvent: (event: TableEvent) => void
	): () => void;
	getObject(target: ObjectRef): Promise<KubeObject>;
	replaceObject(target: ObjectRef, value: KubeObject): Promise<KubeObject>;
	/** Runs a change as a dry run, to show what it would do. */
	preview(target: ObjectRef, change: Change): Promise<Preview>;
	/** The OpenAPI schema of a resource's kind, or null where the server publishes none. */
	schema(resource: ResourceRef): Promise<KindSchema | null>;
	/** JSON merge patch. */
	patchObject(target: ObjectRef, patch: unknown): Promise<KubeObject>;
	deleteObject(target: ObjectRef, options?: DeleteOptions): Promise<void>;
	scale(target: ObjectRef, replicas: number): Promise<void>;
	restart(target: ObjectRef): Promise<void>;
	/** Starts a debug container and says where to connect to it. */
	debug(request: DebugRequest): Promise<DebugSession>;
	/** A higher memory limit for a container of a pod, on the workload that owns it. */
	memoryFix(target: ObjectRef, container: string): Promise<MemoryFix>;
	setMemoryLimit(target: ObjectRef, container: string, memory: string): Promise<void>;
	/** Why each node would refuse a pending pod. */
	scheduling(target: ObjectRef): Promise<NodeFit[]>;
	/** Which pods a Service picks. */
	serviceCheck(target: ObjectRef): Promise<ServiceCheck>;
	/** Applies objects in dependency order, or with `dryRun` shows what would change. */
	apply(request: ApplyRequest): Promise<ApplyItem[]>;
	/** Reads YAML from an http(s) address. */
	fetchManifest(url: string): Promise<string>;
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
	/** Installs or upgrades a release with helm; with `dryRun`, renders it against the cluster only. */
	helmChange(change: HelmChange): Promise<HelmOutcome>;
	/** A chart's default values as YAML. */
	helmChartValues(chart: HelmChartRef): Promise<string>;
	/** Charts on Artifact Hub. */
	helmSearch(query: string): Promise<HubChart[]>;
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

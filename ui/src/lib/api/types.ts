// Mirrors the serialized types of the kubri-core crate. Types in ./generated are written from
// the Rust types by `cargo test` (ts-rs); move a type there instead of copying it here.

import type { KindSchema } from '../schema';
import type { ClusterInfo } from './generated/ClusterInfo';
import type { ContextInfo } from './generated/ContextInfo';
import type { ContextList } from './generated/ContextList';
import type { SnapshotInfo } from './generated/SnapshotInfo';
import type { AccessCheck } from './generated/AccessCheck';
import type { AgentSettings } from './generated/AgentSettings';
import type { Alias } from './generated/Alias';
import type { Amounts } from './generated/Amounts';
import type { ApplyItem } from './generated/ApplyItem';
import type { ApplyOutcome } from './generated/ApplyOutcome';
import type { BenchInfo } from './generated/BenchInfo';
import type { BenchResults } from './generated/BenchResults';
import type { BenchState } from './generated/BenchState';
import type { Capacity } from './generated/Capacity';
import type { CertificateAlert } from './generated/CertificateAlert';
import type { ChangeKind } from './generated/ChangeKind';
import type { Chart } from './generated/Chart';
import type { ChartSeries } from './generated/ChartSeries';
import type { ChartUnit } from './generated/ChartUnit';
import type { Charts } from './generated/Charts';
import type { Column } from './generated/Column';
import type { ColumnSettings } from './generated/ColumnSettings';
import type { ConfigFile } from './generated/ConfigFile';
import type { ConnectionSettings } from './generated/ConnectionSettings';
import type { ContainerImage } from './generated/ContainerImage';
import type { ContainerResources } from './generated/ContainerResources';
import type { ContextSettings } from './generated/ContextSettings';
import type { CustomColumn } from './generated/CustomColumn';
import type { DashboardLink } from './generated/DashboardLink';
import type { Dashboards } from './generated/Dashboards';
import type { DebugSession } from './generated/DebugSession';
import type { DebugSettings } from './generated/DebugSettings';
import type { DeleteOptions } from './generated/DeleteOptions';
import type { DiffSettings } from './generated/DiffSettings';
import type { DrainReport } from './generated/DrainReport';
import type { Drill } from './generated/Drill';
import type { Endpoint } from './generated/Endpoint';
import type { EventInfo } from './generated/EventInfo';
import type { ExplainAnswer } from './generated/ExplainAnswer';
import type { ExplainPrompt } from './generated/ExplainPrompt';
import type { ExplainSettings } from './generated/ExplainSettings';
import type { ExplainSubject } from './generated/ExplainSubject';
import type { FavoriteForward } from './generated/FavoriteForward';
import type { FavoriteStart } from './generated/FavoriteStart';
import type { FieldTest } from './generated/FieldTest';
import type { Finding } from './generated/Finding';
import type { FleetCluster } from './generated/FleetCluster';
import type { Grant } from './generated/Grant';
import type { Graph } from './generated/Graph';
import type { GraphEdge } from './generated/GraphEdge';
import type { GraphNode } from './generated/GraphNode';
import type { Guard } from './generated/Guard';
import type { Health } from './generated/Health';
import type { HelmOutcome } from './generated/HelmOutcome';
import type { HelmRelease } from './generated/HelmRelease';
import type { HelmReleaseDetails } from './generated/HelmReleaseDetails';
import type { HistoryDetail } from './generated/HistoryDetail';
import type { HistoryEntry } from './generated/HistoryEntry';
import type { HistoryPage } from './generated/HistoryPage';
import type { HistorySettings } from './generated/HistorySettings';
import type { Hotkey } from './generated/Hotkey';
import type { HubChart } from './generated/HubChart';
import type { Identity } from './generated/Identity';
import type { Integration } from './generated/Integration';
import type { IntegrationSettings } from './generated/IntegrationSettings';
import type { Integrations } from './generated/Integrations';
import type { Issue } from './generated/Issue';
import type { Jump } from './generated/Jump';
import type { KindPulse } from './generated/KindPulse';
import type { KubeconfigFile } from './generated/KubeconfigFile';
import type { LabelTemplate } from './generated/LabelTemplate';
import type { LatestRelease } from './generated/LatestRelease';
import type { Level } from './generated/Level';
import type { Limits } from './generated/Limits';
import type { LocalClusterTool } from './generated/LocalClusterTool';
import type { LocalClusters } from './generated/LocalClusters';
import type { LocalTerminalInfo } from './generated/LocalTerminalInfo';
import type { LogEvent } from './generated/LogEvent';
import type { LogLevel } from './generated/LogLevel';
import type { LogLine } from './generated/LogLine';
import type { LogSource } from './generated/LogSource';
import type { LokiLogs } from './generated/LokiLogs';
import type { Manager } from './generated/Manager';
import type { MemoryFix } from './generated/MemoryFix';
import type { NewContext } from './generated/NewContext';
import type { NodeFit } from './generated/NodeFit';
import type { NodeRoom } from './generated/NodeRoom';
import type { ObjectRef } from './generated/ObjectRef';
import type { Owner } from './generated/Owner';
import type { PathStep } from './generated/PathStep';
import type { Permissions } from './generated/Permissions';
import type { Plugin } from './generated/Plugin';
import type { PluginInput } from './generated/PluginInput';
import type { PodIssue } from './generated/PodIssue';
import type { PortForwardInfo } from './generated/PortForwardInfo';
import type { Preview } from './generated/Preview';
import type { Pulse } from './generated/Pulse';
import type { QuotaRoom } from './generated/QuotaRoom';
import type { Rbac } from './generated/Rbac';
import type { RecentEvent } from './generated/RecentEvent';
import type { RecorderStatus } from './generated/RecorderStatus';
import type { Reference } from './generated/Reference';
import type { RenderedView } from './generated/RenderedView';
import type { RequestPath } from './generated/RequestPath';
import type { ResourceInfo } from './generated/ResourceInfo';
import type { ResourceRef } from './generated/ResourceRef';
import type { RightSizing } from './generated/RightSizing';
import type { RoleState } from './generated/RoleState';
import type { Row } from './generated/Row';
import type { Rule } from './generated/Rule';
import type { SanitizerReport } from './generated/SanitizerReport';
import type { SanitizerSection } from './generated/SanitizerSection';
import type { SavedSearch } from './generated/SavedSearch';
import type { SearchHit } from './generated/SearchHit';
import type { SearchResult } from './generated/SearchResult';
import type { ServiceCheck } from './generated/ServiceCheck';
import type { Settings } from './generated/Settings';
import type { SizeAdvice } from './generated/SizeAdvice';
import type { SizeReason } from './generated/SizeReason';
import type { Span } from './generated/Span';
import type { SubjectInfo } from './generated/SubjectInfo';
import type { TableEvent } from './generated/TableEvent';
import type { TableScope } from './generated/TableScope';
import type { TerminalMode } from './generated/TerminalMode';
import type { TerminalSettings } from './generated/TerminalSettings';
import type { Threshold } from './generated/Threshold';
import type { Usage } from './generated/Usage';
import type { Utilization } from './generated/Utilization';
import type { ViewSet } from './generated/ViewSet';
import type { WaitingPod } from './generated/WaitingPod';
import type { XrayNode } from './generated/XrayNode';

export type {
	AccessCheck,
	AgentSettings,
	Alias,
	Amounts,
	ApplyItem,
	ApplyOutcome,
	BenchInfo,
	BenchResults,
	BenchState,
	Capacity,
	CertificateAlert,
	ChangeKind,
	Chart,
	ChartSeries,
	ChartUnit,
	Charts,
	ClusterInfo,
	Column,
	ColumnSettings,
	ConfigFile,
	ConnectionSettings,
	ContainerImage,
	ContainerResources,
	ContextInfo,
	ContextList,
	ContextSettings,
	CustomColumn,
	DashboardLink,
	Dashboards,
	DebugSession,
	DebugSettings,
	DeleteOptions,
	DiffSettings,
	DrainReport,
	Drill,
	Endpoint,
	EventInfo,
	ExplainAnswer,
	ExplainPrompt,
	ExplainSettings,
	ExplainSubject,
	FavoriteForward,
	FavoriteStart,
	FieldTest,
	Finding,
	FleetCluster,
	Grant,
	Graph,
	GraphEdge,
	GraphNode,
	Guard,
	Health,
	HelmOutcome,
	HelmRelease,
	HelmReleaseDetails,
	HistoryDetail,
	HistoryEntry,
	HistoryPage,
	HistorySettings,
	Hotkey,
	HubChart,
	Identity,
	Integration,
	IntegrationSettings,
	Integrations,
	Issue,
	Jump,
	KindPulse,
	KubeconfigFile,
	LabelTemplate,
	LatestRelease,
	Level,
	Limits,
	LocalClusterTool,
	LocalClusters,
	LocalTerminalInfo,
	LogEvent,
	LogLevel,
	LogLine,
	LogSource,
	LokiLogs,
	Manager,
	MemoryFix,
	NewContext,
	NodeFit,
	NodeRoom,
	ObjectRef,
	Owner,
	PathStep,
	Permissions,
	Plugin,
	PluginInput,
	PodIssue,
	PortForwardInfo,
	Preview,
	Pulse,
	QuotaRoom,
	Rbac,
	RecentEvent,
	RecorderStatus,
	Reference,
	RenderedView,
	RequestPath,
	ResourceInfo,
	ResourceRef,
	RightSizing,
	RoleState,
	Row,
	Rule,
	SanitizerReport,
	SanitizerSection,
	SavedSearch,
	SearchHit,
	SearchResult,
	ServiceCheck,
	Settings,
	SizeAdvice,
	SizeReason,
	SnapshotInfo,
	Span,
	SubjectInfo,
	TableEvent,
	TableScope,
	TerminalMode,
	TerminalSettings,
	Threshold,
	Usage,
	Utilization,
	ViewSet,
	WaitingPod,
	XrayNode
};

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

export interface Cell {
	name: string;
	value: string;
}

/** A change to try as a server-side dry run. */
export type Change =
	| { type: 'replace'; value: KubeObject }
	| { type: 'patch'; patch: unknown }
	| { type: 'scale'; replicas: number }
	| { type: 'restart' }
	| { type: 'images'; images: ContainerImage[] }
	/** One container's memory limit in a workload's pod template. */
	| { type: 'memory'; container: string; memory: string }
	| { type: 'resources'; containers: ContainerResources[] };

/** A debug container to start, like `kubectl debug`. */
export type DebugRequest =
	/** An ephemeral container in the pod that sees the processes of `container`. */
	| { kind: 'ephemeral'; target: ObjectRef; container: string | null; image: string | null }
	/** A copy of the pod in which `container` sleeps instead of running its command. */
	| { kind: 'copy'; target: ObjectRef; container: string | null }
	/** A privileged pod on the node with a shell in the node's namespaces. */
	| { kind: 'node'; node: string; namespace: string | null; image: string | null };

export interface DrainOptions {
	ignoreDaemonsets?: boolean;
	deleteEmptydirData?: boolean;
	force?: boolean;
	gracePeriodSeconds?: number | null;
}

export type GitOpsAction = 'reconcile' | 'suspend' | 'resume' | 'sync' | 'refresh' | 'autoSyncOff' | 'autoSyncOn';

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

/** A chart by name in a repository URL, as `repo/chart` from a repository added to helm, or `oci://…`. */
export interface HelmChartRef {
	chart: string;
	repo?: string | null;
	/** The latest when empty. */
	version?: string | null;
}

export interface HistoryQuery {
	context?: string;
	namespace?: string;
	kind?: string;
	name?: string;
	uid?: string;
	since?: number;
	until?: number;
	changes?: ChangeKind[];
	text?: string;
	limit?: number;
	before?: number;
}

/** What a local terminal opens with: the namespace shown, null for all namespaces. */
export interface LocalTerminalOptions {
	namespace: string | null;
	cols: number;
	rows: number;
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

export interface LokiRequest {
	target: ObjectRef;
	/** LogQL; the object's own query when left out. */
	query?: string | null;
	container?: string | null;
	seconds: number;
	limit?: number;
}

export type MetricsKind = 'pods' | 'nodes' | 'namespaces';

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

export interface PortForwardRequest {
	target: ObjectRef;
	/** The container port, or the Service port for Services. */
	remotePort: number;
	/** Empty picks a free port. */
	localPort?: number | null;
	address?: string | null;
}

export interface SearchRequest {
	/** Part of the name; `/…/` for a regular expression. */
	text: string;
	/** Empty for the common kinds. */
	kinds: ResourceRef[];
	allKinds: boolean;
	namespace: string | null;
	labels: string;
	fields: FieldTest[];
	/** Empty for the connected context. */
	contexts: string[];
	limit?: number;
}

export interface TerminalOptions {
	container?: string | null;
	mode?: TerminalMode;
	command?: string[] | null;
	cols: number;
	rows: number;
}

/** A link with placeholders like `{namespace}` or `{pod}` that an object fills in. */

export type IntegrationId = 'prometheus' | 'grafana' | 'loki' | 'flux' | 'argocd' | 'certManager';

export interface ChartRange {
	/** Seconds back from `end`. */
	seconds: number;
	/** Unix seconds; now when left out. */
	end?: number;
}

/** Where Explain sends its questions; `none` sends nothing anywhere. */
export type ExplainProvider = 'none' | 'anthropic' | 'openai';

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

/** Where Enter leads from an object: a list of what belongs to it. */

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

export type TerminalEvent =
	| { type: 'ready'; container: string }
	| { type: 'data'; data: Uint8Array }
	| { type: 'exit'; message: string | null };

export interface TerminalSession {
	write(data: string): void;
	resize(cols: number, rows: number): void;
	close(): void;
}

/** The latest release, and whether the desktop app can install it itself. */
export interface UpdateCheck extends LatestRelease {
	/** Kubri downloads, checks and installs it, then starts again. Only in the desktop app. */
	installable?: boolean;
	/** Why it cannot, when the release is newer: the download page is the way then. */
	manual?: string | null;
}

/** How far installing an update has come. */
export type UpdateProgress = { type: 'download'; done: number; total: number | null } | { type: 'install' };

export interface LoggerSettings {
	/** Lines to start with. */
	tail: number;
	/** How far back to start, in seconds; -1 for the tail only. */
	sinceSeconds: number;
	textWrap: boolean;
	showTime: boolean;
	disableAutoscroll: boolean;
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

/** The operations both the desktop app and kubri-server offer to the UI. */
/** How Kubri shows a custom resource kind: from a YAML file, built in or the user's. */
export interface KindView {
	group: string;
	kind: string;
	columns: CustomColumn[];
	health: {
		path: string;
		equals?: string | null;
		notEquals?: string | null;
		matches?: string | null;
		exists?: boolean | null;
		state: Health;
		message?: string | null;
	}[];
	details: { title: string; fields: CustomColumn[] }[];
	actions: { name: string; shortcut: string; description: string; command: string; args: string[] }[];
	relations: { kind: string; name: string; namespace?: string | null }[];
	/** The file it came from, `built-in: flux.yaml` for Kubri's own. */
	source: string;
}

export interface Backend {
	contexts(): Promise<ContextList>;
	reloadContexts(): Promise<ContextList>;
	/** The contexts of a kubeconfig the user picked, and why any cannot be added. */
	inspectKubeconfig(text: string): Promise<NewContext[]>;
	/** Adds contexts from a kubeconfig to Kubri's own list; the user's files stay as they are. */
	addContexts(text: string, names: string[]): Promise<ContextList>;
	/** Removes a context added in Kubri. */
	removeContext(name: string): Promise<ContextList>;
	/** Every kubeconfig file Kubri reads: your own, then the ones chosen in Kubri. */
	kubeconfigFiles(): Promise<KubeconfigFile[]>;
	/** Asks for kubeconfig files in the system's file dialog; null where there is none (the browser). */
	chooseKubeconfigFiles(): Promise<string[] | null>;
	/** Reads these kubeconfig files where they are from now on. Desktop only. */
	addKubeconfigFiles(paths: string[]): Promise<ContextList>;
	/** Stops reading a file chosen in Kubri. Desktop only. */
	removeKubeconfigFile(path: string): Promise<ContextList>;
	switchContext(name: string): Promise<ClusterInfo>;
	/** Stops waiting for a switch under way; the current connection stays. */
	cancelSwitch(): Promise<void>;
	/** Saves a namespace, or the whole cluster for null, to a snapshot file. Returns where the
	 * desktop app saved it; the browser downloads it and returns null. */
	takeSnapshot(namespace: string | null): Promise<string | null>;
	/** Opens a snapshot file in place of a cluster, read-only. */
	openSnapshot(file: File): Promise<ClusterInfo>;
	/** Leaves the open snapshot for no context, when there is none to go back to. */
	closeSnapshot(): Promise<void>;
	resources(): Promise<ResourceInfo[]>;
	/** Runs discovery again, after a switch to a kept connection. */
	refreshResources(): Promise<ResourceInfo[]>;
	/** Whether each check is allowed; null where the API server cannot say. */
	access(checks: AccessCheck[], fresh?: boolean): Promise<(boolean | null)[]>;
	whoAmI(): Promise<Identity | null>;
	/** All namespaces, or those the kubeconfig and settings name where listing them is not allowed. */
	readableNamespaces(): Promise<string[]>;
	settings(): Promise<SettingsReply>;
	setContextSettings(name: string, settings: ContextSettings): Promise<Settings>;
	setDiffSettings(settings: DiffSettings): Promise<Settings>;
	setHistorySettings(settings: HistorySettings): Promise<Settings>;
	setConnectionSettings(settings: ConnectionSettings): Promise<Settings>;
	setTerminalSettings(settings: TerminalSettings): Promise<Settings>;
	setLabelTemplates(templates: LabelTemplate[]): Promise<Settings>;
	/** What changed, newest first, with the times Kubri was watching. */
	history(query: HistoryQuery): Promise<HistoryPage>;
	/** One change with the object before and after it. */
	historyDetail(id: number): Promise<HistoryDetail>;
	/** How lists show a kind, by `group/Kind`; the defaults forget it. */
	setColumnSettings(kind: string, columns: ColumnSettings): Promise<Settings>;
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
	/** The object in its own context, also one Kubri is not connected to, for Compare. */
	getObjectElsewhere(target: ObjectRef): Promise<KubeObject>;
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
	/** A Flux or Argo CD action, as their CLIs run it. */
	gitops(target: ObjectRef, action: GitOpsAction): Promise<void>;
	/** Asks cert-manager to issue a Certificate again now, like `cmctl renew`. */
	renewCertificate(target: ObjectRef): Promise<void>;
	/** Objects by name, labels and fields, over kinds and contexts. */
	search(request: SearchRequest): Promise<SearchResult>;
	/** Replaces the searches kept in the sidebar. */
	setSavedSearches(searches: SavedSearch[]): Promise<Settings>;
	/** How a context is doing, without connecting to it. */
	fleetCluster(context: string): Promise<FleetCluster>;
	/** Containers whose requests or limits are far from what they use. */
	rightSizing(namespace: string | null): Promise<RightSizing>;
	/** Sets one container's requests and limits in a workload's pod template. */
	/** Sets requests and limits of containers in a workload's pod template, in one patch; null removes one. */
	setResources(target: ObjectRef, containers: ContainerResources[]): Promise<void>;
	/** The room a workload has on its nodes and in its namespace's quotas. */
	capacity(target: ObjectRef): Promise<Capacity>;
	/** The Flux or Argo CD object that manages an object, if any. */
	managedBy(target: ObjectRef): Promise<Manager | null>;
	/** The views for custom resource kinds, read again from their files. */
	views(): Promise<ViewSet>;
	/** The health and sections the kind's view gives `object`; null without a view. */
	renderView(resource: ResourceRef, object: unknown): Promise<RenderedView | null>;
	setImages(target: ObjectRef, images: ContainerImage[]): Promise<void>;
	/** Rolls a ReplicaSet's Deployment, or a ControllerRevision's StatefulSet or DaemonSet, back to it; returns what it rolled back, like `Deployment web`. */
	rollback(target: ObjectRef): Promise<string>;
	events(namespace: string | null, uid: string): Promise<EventInfo[]>;
	metrics(kind: MetricsKind, namespace: string | null): Promise<Usage[]>;
	/** Streams logs of a pod, workload or Service and returns a function that stops it. */
	streamLogs(target: ObjectRef, options: LogOptions, onEvent: (event: LogEvent) => void): () => void;
	/** Opens a shell or attaches to a container. */
	openTerminal(target: ObjectRef, options: TerminalOptions, onEvent: (event: TerminalEvent) => void): TerminalSession;
	portForwards(): Promise<PortForwardInfo[]>;
	startPortForward(request: PortForwardRequest): Promise<PortForwardInfo>;
	/** Starts the connected context's favorite forwards that are not running. */
	startFavoriteForwards(): Promise<FavoriteStart[]>;
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
	/** The tools the cluster runs; `fresh` looks again and asks each whether it answers. */
	integrations(fresh: boolean): Promise<Integrations>;
	/** Charts of a pod, workload, node or namespace from Prometheus. */
	charts(target: ObjectRef, range: ChartRange): Promise<Charts>;
	/** CPU and memory of a namespace, or the cluster, from Prometheus. */
	usageHistory(namespace: string | null, seconds: number): Promise<Charts>;
	/** Grafana dashboards that show an object, its logs in Explore first. */
	dashboards(target: ObjectRef): Promise<Dashboards>;
	/** An object's logs in Grafana's Explore: a pod's, a workload's, a Service's, or its namespace's. */
	grafanaLogs(target: ObjectRef): Promise<DashboardLink>;
	/** Where the browser reaches a tool; a port-forward for one on a Service. */
	integrationUrl(id: IntegrationId): Promise<string>;
	/** Log history from Loki. */
	lokiLogs(request: LokiRequest): Promise<LokiLogs>;
	/** The XRay trees of every object of `resource`. */
	xray(resource: ResourceRef, namespace: string | null): Promise<XrayNode[]>;
	/** The objects of a namespace, or all, as a graph, with the request paths of its Ingresses. */
	graph(namespace: string | null): Promise<Graph>;
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
	/** Asks GitHub for the latest release of Kubri, at most every few hours. */
	latestRelease(): Promise<LatestRelease>;
	/** The latest release, and in the desktop app whether it can install it. */
	checkUpdate(): Promise<UpdateCheck>;
	/** Installs the update `checkUpdate` found and starts Kubri again. Only in the desktop app. */
	installUpdate(onProgress: (progress: UpdateProgress) => void): Promise<void>;
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
	/** The tools that can make a cluster on this computer, and whether Kubri may run them. */
	localClusters(): Promise<LocalClusters>;
	/** Makes a cluster on this computer, showing the tool's progress like a terminal. */
	createLocalCluster(
		request: { tool: LocalClusterTool; cols: number; rows: number },
		onEvent: (event: TerminalEvent) => void
	): TerminalSession;
	/** Whether a local terminal can be opened for the connected context. */
	localTerminalInfo(): Promise<LocalTerminalInfo>;
	/** Opens a shell on this computer, or on the server in the web app. */
	openLocalTerminal(options: LocalTerminalOptions, onEvent: (event: TerminalEvent) => void): TerminalSession;
	/** The desktop app, where port-forwards and plugins run on this computer. */
	desktop: boolean;
	/** Opens a web address in a new tab, or in the default browser from the desktop app. */
	openUrl(url: string): Promise<void>;
	/** Saves text as a file. Returns the path when the desktop app saved it to Downloads. */
	saveText(name: string, text: string): Promise<string | null>;
}


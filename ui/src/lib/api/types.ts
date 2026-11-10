// Mirrors the serialized types of the kubri-core crate. Types in ./generated are written from
// the Rust types by `cargo test` (ts-rs); move a type there instead of copying it here.

import type { KindSchema } from '../schema';
import type { AccessCheck } from './generated/AccessCheck';
import type { AgentCall } from './generated/AgentCall';
import type { AgentConnection } from './generated/AgentConnection';
import type { AgentRequest } from './generated/AgentRequest';
import type { AgentSettings } from './generated/AgentSettings';
import type { Alias } from './generated/Alias';
import type { Amounts } from './generated/Amounts';
import type { ApplyItem } from './generated/ApplyItem';
import type { ApplyOutcome } from './generated/ApplyOutcome';
import type { ApplyRequest } from './generated/ApplyRequest';
import type { BenchInfo } from './generated/BenchInfo';
import type { BenchRequest } from './generated/BenchRequest';
import type { BenchResults } from './generated/BenchResults';
import type { BenchState } from './generated/BenchState';
import type { Capacity } from './generated/Capacity';
import type { Cell } from './generated/Cell';
import type { CertificateAlert } from './generated/CertificateAlert';
import type { Change } from './generated/Change';
import type { ChangeKind } from './generated/ChangeKind';
import type { Chart } from './generated/Chart';
import type { ChartRange } from './generated/ChartRange';
import type { ChartSeries } from './generated/ChartSeries';
import type { ChartUnit } from './generated/ChartUnit';
import type { Charts } from './generated/Charts';
import type { ClusterInfo } from './generated/ClusterInfo';
import type { Column } from './generated/Column';
import type { ColumnSettings } from './generated/ColumnSettings';
import type { ConfigFile } from './generated/ConfigFile';
import type { ConnectionSettings } from './generated/ConnectionSettings';
import type { ContainerImage } from './generated/ContainerImage';
import type { ContainerResources } from './generated/ContainerResources';
import type { ContextInfo } from './generated/ContextInfo';
import type { ContextList } from './generated/ContextList';
import type { ContextSettings } from './generated/ContextSettings';
import type { CustomColumn } from './generated/CustomColumn';
import type { DashboardLink } from './generated/DashboardLink';
import type { Dashboards } from './generated/Dashboards';
import type { DebugRequest } from './generated/DebugRequest';
import type { DebugSession } from './generated/DebugSession';
import type { DebugSettings } from './generated/DebugSettings';
import type { DeleteOptions } from './generated/DeleteOptions';
import type { DiffSettings } from './generated/DiffSettings';
import type { DrainOptions } from './generated/DrainOptions';
import type { DrainReport } from './generated/DrainReport';
import type { Drill } from './generated/Drill';
import type { Endpoint } from './generated/Endpoint';
import type { EventInfo } from './generated/EventInfo';
import type { ExplainAnswer } from './generated/ExplainAnswer';
import type { ExplainPrompt } from './generated/ExplainPrompt';
import type { ExplainProvider } from './generated/ExplainProvider';
import type { ExplainSettings } from './generated/ExplainSettings';
import type { ExplainSubject } from './generated/ExplainSubject';
import type { FavoriteForward } from './generated/FavoriteForward';
import type { FavoriteStart } from './generated/FavoriteStart';
import type { FieldTest } from './generated/FieldTest';
import type { Finding } from './generated/Finding';
import type { FleetCluster } from './generated/FleetCluster';
import type { GitOpsAction } from './generated/GitOpsAction';
import type { Grant } from './generated/Grant';
import type { Graph } from './generated/Graph';
import type { GraphEdge } from './generated/GraphEdge';
import type { GraphNode } from './generated/GraphNode';
import type { Guard } from './generated/Guard';
import type { Health } from './generated/Health';
import type { HelmChange } from './generated/HelmChange';
import type { HelmChartRef } from './generated/HelmChartRef';
import type { HelmOutcome } from './generated/HelmOutcome';
import type { HelmRelease } from './generated/HelmRelease';
import type { HelmReleaseDetails } from './generated/HelmReleaseDetails';
import type { HistoryDetail } from './generated/HistoryDetail';
import type { HistoryEntry } from './generated/HistoryEntry';
import type { HistoryPage } from './generated/HistoryPage';
import type { HistoryQuery } from './generated/HistoryQuery';
import type { HistorySettings } from './generated/HistorySettings';
import type { Hotkey } from './generated/Hotkey';
import type { HubChart } from './generated/HubChart';
import type { Identity } from './generated/Identity';
import type { Integration } from './generated/Integration';
import type { IntegrationSettings } from './generated/IntegrationSettings';
import type { Integrations } from './generated/Integrations';
import type { Issue } from './generated/Issue';
import type { Jump } from './generated/Jump';
import type { K9sConfig } from './generated/K9sConfig';
import type { KindPulse } from './generated/KindPulse';
import type { KindView } from './generated/KindView';
import type { KubeconfigFile } from './generated/KubeconfigFile';
import type { LabelTemplate } from './generated/LabelTemplate';
import type { LatestRelease } from './generated/LatestRelease';
import type { Level } from './generated/Level';
import type { Limits } from './generated/Limits';
import type { LocalClusterTool } from './generated/LocalClusterTool';
import type { LocalClusters } from './generated/LocalClusters';
import type { LocalTerminalInfo } from './generated/LocalTerminalInfo';
import type { LocalTerminalOptions } from './generated/LocalTerminalOptions';
import type { LogEvent } from './generated/LogEvent';
import type { LogLevel } from './generated/LogLevel';
import type { LogLine } from './generated/LogLine';
import type { LogOptions } from './generated/LogOptions';
import type { LogSource } from './generated/LogSource';
import type { LoggerSettings } from './generated/LoggerSettings';
import type { LokiLogs } from './generated/LokiLogs';
import type { LokiRequest } from './generated/LokiRequest';
import type { Manager } from './generated/Manager';
import type { MemoryFix } from './generated/MemoryFix';
import type { MetricsKind } from './generated/MetricsKind';
import type { NewContext } from './generated/NewContext';
import type { NodeFit } from './generated/NodeFit';
import type { NodeRoom } from './generated/NodeRoom';
import type { ObjectRef } from './generated/ObjectRef';
import type { Owner } from './generated/Owner';
import type { PathStep } from './generated/PathStep';
import type { Permissions } from './generated/Permissions';
import type { Plugin } from './generated/Plugin';
import type { PluginInput } from './generated/PluginInput';
import type { PluginRun } from './generated/PluginRun';
import type { PodIssue } from './generated/PodIssue';
import type { PortForwardInfo } from './generated/PortForwardInfo';
import type { PortForwardRequest } from './generated/PortForwardRequest';
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
import type { SearchRequest } from './generated/SearchRequest';
import type { SearchResult } from './generated/SearchResult';
import type { ServiceCheck } from './generated/ServiceCheck';
import type { Settings } from './generated/Settings';
import type { SettingsReply } from './generated/SettingsReply';
import type { SizeAdvice } from './generated/SizeAdvice';
import type { SizeReason } from './generated/SizeReason';
import type { SnapshotInfo } from './generated/SnapshotInfo';
import type { Span } from './generated/Span';
import type { SubjectInfo } from './generated/SubjectInfo';
import type { TableEvent } from './generated/TableEvent';
import type { TableScope } from './generated/TableScope';
import type { TerminalMode } from './generated/TerminalMode';
import type { TerminalOptions } from './generated/TerminalOptions';
import type { TerminalSettings } from './generated/TerminalSettings';
import type { Threshold } from './generated/Threshold';
import type { Usage } from './generated/Usage';
import type { Utilization } from './generated/Utilization';
import type { ViewSet } from './generated/ViewSet';
import type { WaitingPod } from './generated/WaitingPod';
import type { XrayNode } from './generated/XrayNode';

export type {
	AccessCheck,
	AgentCall,
	AgentConnection,
	AgentRequest,
	AgentSettings,
	Alias,
	Amounts,
	ApplyItem,
	ApplyOutcome,
	ApplyRequest,
	BenchInfo,
	BenchRequest,
	BenchResults,
	BenchState,
	Capacity,
	Cell,
	CertificateAlert,
	Change,
	ChangeKind,
	Chart,
	ChartRange,
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
	DebugRequest,
	DebugSession,
	DebugSettings,
	DeleteOptions,
	DiffSettings,
	DrainOptions,
	DrainReport,
	Drill,
	Endpoint,
	EventInfo,
	ExplainAnswer,
	ExplainPrompt,
	ExplainProvider,
	ExplainSettings,
	ExplainSubject,
	FavoriteForward,
	FavoriteStart,
	FieldTest,
	Finding,
	FleetCluster,
	GitOpsAction,
	Grant,
	Graph,
	GraphEdge,
	GraphNode,
	Guard,
	Health,
	HelmChange,
	HelmChartRef,
	HelmOutcome,
	HelmRelease,
	HelmReleaseDetails,
	HistoryDetail,
	HistoryEntry,
	HistoryPage,
	HistoryQuery,
	HistorySettings,
	Hotkey,
	HubChart,
	Identity,
	Integration,
	IntegrationSettings,
	Integrations,
	Issue,
	Jump,
	K9sConfig,
	KindPulse,
	KindView,
	KubeconfigFile,
	LabelTemplate,
	LatestRelease,
	Level,
	Limits,
	LocalClusterTool,
	LocalClusters,
	LocalTerminalInfo,
	LocalTerminalOptions,
	LogEvent,
	LogLevel,
	LogLine,
	LogOptions,
	LogSource,
	LoggerSettings,
	LokiLogs,
	LokiRequest,
	Manager,
	MemoryFix,
	MetricsKind,
	NewContext,
	NodeFit,
	NodeRoom,
	ObjectRef,
	Owner,
	PathStep,
	Permissions,
	Plugin,
	PluginInput,
	PluginRun,
	PodIssue,
	PortForwardInfo,
	PortForwardRequest,
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
	SearchRequest,
	SearchResult,
	ServiceCheck,
	Settings,
	SettingsReply,
	SizeAdvice,
	SizeReason,
	SnapshotInfo,
	Span,
	SubjectInfo,
	TableEvent,
	TableScope,
	TerminalMode,
	TerminalOptions,
	TerminalSettings,
	Threshold,
	Usage,
	Utilization,
	ViewSet,
	WaitingPod,
	XrayNode
};

export type IntegrationId = Integration['id'];

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


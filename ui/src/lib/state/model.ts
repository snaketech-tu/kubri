/** The shapes and helpers the app state is built from, without any state of its own. */
import type { Fill } from '../resources';
import type {
	AccessCheck,
	BenchInfo,
	GitOpsAction,
	HelmRelease,
	MemoryFix,
	ObjectRef,
	Plugin,
	PluginRun,
	PortForwardInfo,
	ResourceInfo,
	ResourceRef,
	TableScope,
	TerminalMode
} from '../api';

export const ALL_NAMESPACES = '';

export type DetailsTab = 'describe' | 'events' | 'yaml' | 'metrics' | 'logs' | 'shell';

export interface Details {
	target: ObjectRef;
	resource: ResourceInfo;
	tab: DetailsTab;
	editing: boolean;
	/** Start the logs with the previous container instance. */
	previous?: boolean;
	/** How the shell tab connects. */
	mode?: TerminalMode;
	/** The container the shell tab connects to, such as a debug container just started. */
	container?: string;
	/** Show the decoded values of a Secret, like k9s `x`. */
	decoded?: boolean;
}

export type DetailsExtra = Partial<Pick<Details, 'editing' | 'previous' | 'mode' | 'decoded' | 'container'>>;

/** A resource list narrowed to what belongs to one object, like the pods of a Deployment. */
export interface ListScope {
	/** Such as `Pods of Deployment web`. */
	title: string;
	scope: TableScope;
	/** Keeps only what this UID owns. */
	ownerUid: string | null;
}

/** A place in Kubri, for going back and forth like k9s `[`, `]` and `-`. */
export interface View {
	page: Page;
	resource: ResourceRef | null;
	namespace: string;
	scope: ListScope | null;
	xray: ResourceRef | null;
}

export const PAGES = [
	'resources',
	'portforwards',
	'benchmarks',
	'pulses',
	'xray',
	'graph',
	'rbac',
	'helm',
	'create',
	'sanitizer',
	'integrations',
	'rightsizing',
	'fleet',
	'search',
	'changes',
	'config',
	'tools',
	'kinds'
] as const;

export type Page = (typeof PAGES)[number];

export type Dialog =
	| { kind: 'delete'; resource: ResourceInfo; targets: ObjectRef[]; statuses?: Record<string, string> }
	| { kind: 'scale'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'restart'; resource: ResourceInfo; target: ObjectRef }
	/** Several marked objects at once. */
	| { kind: 'scaleMany'; resource: ResourceInfo; targets: ObjectRef[] }
	| { kind: 'restartMany'; resource: ResourceInfo; targets: ObjectRef[] }
	| { kind: 'labels'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'portforward'; resource: ResourceInfo; target: ObjectRef }
	/** A forward to an object chosen in the dialog: namespace, then a workload, service or pod. */
	| { kind: 'newForward' }
	| { kind: 'benchmark'; forward: PortForwardInfo; preset?: BenchInfo }
	| { kind: 'drain'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'helmRollback'; release: HelmRelease; revision: number; done: () => void }
	| { kind: 'helmUninstall'; release: HelmRelease; done: () => void }
	| { kind: 'plugin'; plugin: Plugin; run: PluginRun; variables: Record<string, string> }
	| { kind: 'trigger'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'suspend'; resource: ResourceInfo; target: ObjectRef; suspend: boolean }
	| { kind: 'image'; resource: ResourceInfo; target: ObjectRef }
	| { kind: 'rollback'; resource: ResourceInfo; target: ObjectRef; owner?: { kind: string; plural: string; name: string }; revision?: number }
	/** A Flux or Argo CD action; `autoSync` turns Argo CD's auto-sync off or on, as it is now. */
	| { kind: 'gitops'; resource: ResourceInfo; target: ObjectRef; action: GitOpsAction | 'autoSync' }
	/**
	 * The requests and limits of a workload's containers, with the room on its nodes. `fill`
	 * prefills values, as right-sizing advice suggests them; the cursor starts at the first.
	 */
	| {
			kind: 'resources';
			resource: ResourceInfo;
			target: ObjectRef;
			fill?: Fill[];
			/** Why, as the advice says it. */
			note?: string;
	  }
	/** Has cert-manager issue a Certificate again now. */
	| { kind: 'renew'; resource: ResourceInfo; target: ObjectRef }
	/** A debug container in a pod, a debug copy of it, or a shell on a node. */
	| { kind: 'debug'; on: 'pod' | 'node'; target: ObjectRef; way?: 'ephemeral' | 'copy'; container?: string }
	/** The same object in another context or namespace, side by side. */
	| { kind: 'compare'; resource: ResourceInfo; target: ObjectRef }
	/** The Grafana dashboards and the context's links for an object. */
	| { kind: 'grafana'; target: ObjectRef }
	/** A higher memory limit for a container, on the workload that owns its pod. */
	| { kind: 'memory'; resource: ResourceInfo; target: ObjectRef; fix: MemoryFix }
	/** The kubectl commands for an object's actions, to copy. */
	| { kind: 'kubectl'; title: string; commands: { label: string; command: string }[] }
	/** Explains an action RBAC refuses, and what would allow it. */
	| { kind: 'forbidden'; action: string; check: AccessCheck; target: ObjectRef | null }
	/** Saves a namespace or the cluster to a snapshot file, or opens one; `namespace` null is the cluster. */
	| { kind: 'snapshot'; namespace: string | null };

/** One change made in this session. */
export interface Activity {
	id: number;
	at: number;
	context: string;
	text: string;
	ok: boolean;
	/** Sets things back, where Kubernetes allows it. */
	undo?: () => Promise<unknown>;
	undone: boolean;
}

export interface Toast {
	id: number;
	tone: 'ok' | 'bad';
	text: string;
}


export function sameResource(a: ResourceRef, b: ResourceRef) {
	return a.group === b.group && a.version === b.version && a.plural === b.plural;
}

export function sameRef(a: ResourceRef | null, b: ResourceRef | null) {
	return a === b || (!!a && !!b && sameResource(a, b));
}

export function sameView(a: View, b: View) {
	return (
		a.page === b.page &&
		a.namespace === b.namespace &&
		sameRef(a.resource, b.resource) &&
		sameRef(a.xray, b.xray) &&
		JSON.stringify(a.scope) === JSON.stringify(b.scope)
	);
}

export function sameObject(a: ObjectRef, b: ObjectRef) {
	return sameResource(a.resource, b.resource) && a.namespace === b.namespace && a.name === b.name;
}

/** The served resource type `ref` names, ignoring the version if the cluster serves another. */
export function findResource(resources: ResourceInfo[], ref: ResourceRef): ResourceInfo | undefined {
	return (
		resources.find((r) => sameResource(r, ref)) ??
		resources.find((r) => r.group === ref.group && r.plural === ref.plural)
	);
}

export function refOf(resource: ResourceRef): ResourceRef {
	return { group: resource.group, version: resource.version, plural: resource.plural };
}

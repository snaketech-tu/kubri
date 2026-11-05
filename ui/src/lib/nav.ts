import type { IconName } from './components/Icon.svelte';
import type { ResourceInfo, ResourceRef } from './api';

export interface NavItem {
	plural: string;
	/** Plural, as the sidebar and the list's title show it. */
	label: string;
	icon: IconName;
}

/** The views k9s users jump to most, grouped and in this order. Core resources only. */
export const NAV_SECTIONS: { title: string; items: NavItem[] }[] = [
	{
		title: 'Workloads',
		items: [
			{ plural: 'pods', label: 'Pods', icon: 'pod' },
			{ plural: 'deployments', label: 'Deployments', icon: 'deployment' },
			{ plural: 'statefulsets', label: 'StatefulSets', icon: 'statefulset' },
			{ plural: 'daemonsets', label: 'DaemonSets', icon: 'daemonset' },
			{ plural: 'jobs', label: 'Jobs', icon: 'job' },
			{ plural: 'cronjobs', label: 'CronJobs', icon: 'cronjob' }
		]
	},
	{
		title: 'Network',
		items: [
			{ plural: 'services', label: 'Services', icon: 'service' },
			{ plural: 'ingresses', label: 'Ingresses', icon: 'ingress' },
			{ plural: 'networkpolicies', label: 'Network policies', icon: 'access' }
		]
	},
	{
		title: 'Config and storage',
		items: [
			{ plural: 'configmaps', label: 'ConfigMaps', icon: 'config' },
			{ plural: 'secrets', label: 'Secrets', icon: 'secret' },
			{ plural: 'persistentvolumeclaims', label: 'Volume claims', icon: 'volume' }
		]
	},
	{
		title: 'Cluster',
		items: [
			{ plural: 'nodes', label: 'Nodes', icon: 'node' },
			{ plural: 'namespaces', label: 'Namespaces', icon: 'namespace' },
			{ plural: 'events', label: 'Events', icon: 'event' },
			{ plural: 'customresourcedefinitions', label: 'CRDs', icon: 'resource' }
		]
	}
];

/** The pages beyond the lists, as the sidebar and the Tools page show them. */
export const TOOLS = [
	{ id: 'xray', label: 'Topology', icon: 'topology', command: ':xray', detail: 'What each object depends on, as trees or as one graph' },
	{ id: 'changes', label: 'What changed', icon: 'history', command: ':changes', detail: 'Rollouts, scaling, config edits and warnings, as a timeline' },
	{ id: 'helm', label: 'Helm', icon: 'helm', command: ':helm', detail: 'Releases with their values, history and rollback' },
	{ id: 'portforwards', label: 'Port-forwards', icon: 'forward', command: ':pf', detail: 'Running port-forwards, and favorites that start on their own' },
	{ id: 'rbac', label: 'Access', icon: 'access', command: ':rbac', detail: 'Who may do what, and what you may do yourself' },
	{ id: 'sanitizer', label: 'Checks', icon: 'sanitizer', command: ':popeye', detail: 'Common mistakes and risky settings, with a grade' },
	{ id: 'rightsizing', label: 'Right-sizing', icon: 'sizing', command: ':rightsizing', detail: 'Containers that request much more or less than they use' },
	{ id: 'benchmarks', label: 'Benchmarks', icon: 'benchmark', command: ':bench', detail: 'HTTP load tests through a port-forward' }
] as const satisfies readonly { id: string; label: string; icon: IconName; command: string; detail: string }[];

export type ToolId = (typeof TOOLS)[number]['id'];

/** The tools the sidebar lists; the others are a click away on the Tools page. */
export const SIDEBAR_TOOLS = 4;

/** The pages that belong to a tool: Topology has its trees and its graph. */
export function toolOf(page: string): ToolId | null {
	if (page === 'graph') return 'xray';
	return TOOLS.find((t) => t.id === page)?.id ?? null;
}

/** The kinds the cluster serves, by API group with the core group first, that match `filter`. */
export function kindGroups(resources: ResourceInfo[], filter: string): { group: string; kinds: ResourceInfo[] }[] {
	const words = filter.trim().toLowerCase().split(/\s+/).filter(Boolean);
	const matches = (r: ResourceInfo) => {
		const text = [r.kind, r.plural, r.group, ...r.shortNames].join(' ').toLowerCase();
		return words.every((w) => text.includes(w));
	};
	const groups = new Map<string, ResourceInfo[]>();
	for (const r of resources) {
		if (!r.verbs.includes('watch') || r.plural.includes('/') || !matches(r)) continue;
		// The same kind served in two versions shows once, in the version Kubri lists.
		const kinds = groups.get(r.group) ?? [];
		if (!kinds.some((k) => k.plural === r.plural)) kinds.push(r);
		groups.set(r.group, kinds);
	}
	return [...groups]
		.sort(([a], [b]) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)))
		.map(([group, kinds]) => ({ group, kinds: kinds.sort((a, b) => a.kind.localeCompare(b.kind)) }));
}

const BUILT_IN_GROUPS = ['', 'apps', 'batch', 'networking.k8s.io', 'events.k8s.io'];

/** Where a resource sits in the sidebar, for the breadcrumb and the list's title. */
export function navPlace(resource: ResourceRef & { kind: string }): { section: string; label: string } {
	if (BUILT_IN_GROUPS.includes(resource.group)) {
		for (const section of NAV_SECTIONS) {
			const item = section.items.find((i) => i.plural === resource.plural);
			if (item) return { section: section.title, label: item.label };
		}
	}
	return { section: 'All kinds', label: pluralKind(resource.kind, resource.plural) };
}

/** The kind in its plural, cased like the kind: NetworkPolicy and networkpolicies make NetworkPolicies. */
export function pluralKind(kind: string, plural: string): string {
	const k = kind.toLowerCase();
	let n = 0;
	while (n < k.length && n < plural.length && k[n] === plural[n]) n++;
	// A plural that is not the kind with a changed ending, such as a short name, keeps the kind.
	return n >= k.length - 1 ? kind.slice(0, n) + plural.slice(n) : kind;
}

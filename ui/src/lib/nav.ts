import type { IconName } from './components/Icon.svelte';
import type { ResourceRef } from './api';

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
		title: 'Cluster',
		items: [
			{ plural: 'nodes', label: 'Nodes', icon: 'node' },
			{ plural: 'namespaces', label: 'Namespaces', icon: 'namespace' },
			{ plural: 'services', label: 'Services', icon: 'service' },
			{ plural: 'ingresses', label: 'Ingresses', icon: 'ingress' },
			{ plural: 'configmaps', label: 'ConfigMaps', icon: 'config' },
			{ plural: 'secrets', label: 'Secrets', icon: 'secret' },
			{ plural: 'persistentvolumeclaims', label: 'Volume claims', icon: 'volume' },
			{ plural: 'events', label: 'Events', icon: 'event' }
		]
	}
];

const BUILT_IN_GROUPS = ['', 'apps', 'batch', 'networking.k8s.io', 'events.k8s.io'];

/** Where a resource sits in the sidebar, for the breadcrumb and the list's title. */
export function navPlace(resource: ResourceRef & { kind: string }): { section: string; label: string } {
	if (BUILT_IN_GROUPS.includes(resource.group)) {
		for (const section of NAV_SECTIONS) {
			const item = section.items.find((i) => i.plural === resource.plural);
			if (item) return { section: section.title, label: item.label };
		}
	}
	return { section: 'All resources', label: pluralKind(resource.kind, resource.plural) };
}

/** The kind in its plural, cased like the kind: NetworkPolicy and networkpolicies make NetworkPolicies. */
export function pluralKind(kind: string, plural: string): string {
	const k = kind.toLowerCase();
	let n = 0;
	while (n < k.length && n < plural.length && k[n] === plural[n]) n++;
	// A plural that is not the kind with a changed ending, such as a short name, keeps the kind.
	return n >= k.length - 1 ? kind.slice(0, n) + plural.slice(n) : kind;
}

import type { ObjectRef, PortForwardInfo, ResourceInfo, SearchHit } from './api';
import type { SelectOption } from './select';
import { relative } from './format';
import { t } from './i18n';

/** How the last connection through a forward ended, for its row; null before the first one ends. */
export function lastEnd(forward: Pick<PortForwardInfo, 'lastEnd'>, now: number): string | null {
	const end = forward.lastEnd;
	if (!end) return null;
	const when = relative(end.at, now);
	switch (end.by) {
		case 'pod':
			return t('Last connection closed by the pod, {when}', { when });
		case 'client':
			return t('Last connection closed by the client, {when}', { when });
		case 'error':
			return null;
	}
}

/** The kinds a new forward can start from, in the order the dialog lists them; ReplicaSets are left out. */
const TARGET_KINDS = [
	{ group: 'apps', plural: 'deployments' },
	{ group: 'apps', plural: 'statefulsets' },
	{ group: 'apps', plural: 'daemonsets' },
	{ group: '', plural: 'services' },
	{ group: '', plural: 'pods' }
];

/** The kinds of the cluster that a forward can start from, workloads first. */
export function forwardKinds(resources: ResourceInfo[]): ResourceInfo[] {
	return TARGET_KINDS.flatMap(({ group, plural }) => resources.filter((r) => r.group === group && r.plural === plural).slice(0, 1));
}

/** The value of a target in the dialog's select box. */
export function targetKey(target: Pick<ObjectRef, 'resource' | 'name'>): string {
	return `${target.resource.group}/${target.resource.plural}/${target.name}`;
}

/** Found objects as options, grouped by kind in the order of `kinds`, by name within a kind. */
export function targetOptions(hits: SearchHit[], kinds: ResourceInfo[]): SelectOption<string>[] {
	return kinds.flatMap((kind) =>
		hits
			.filter((h) => h.object.resource.group === kind.group && h.object.resource.plural === kind.plural)
			.map((h) => h.object.name)
			.sort((a, b) => a.localeCompare(b))
			.map((name) => ({ value: targetKey({ resource: kind, name }), label: name, group: kind.kind }))
	);
}

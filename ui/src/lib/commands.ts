import type { ContextInfo, ResourceInfo } from './api';
import { xrayResources } from './xray';

export type Action =
	| { type: 'resource'; resource: ResourceInfo; namespace?: string }
	| { type: 'context'; name: string }
	| { type: 'namespace'; name: string }
	| { type: 'portforwards' }
	| { type: 'pulses'; namespace?: string }
	| { type: 'xray'; resource: ResourceInfo; namespace?: string }
	| { type: 'rbac'; namespace?: string }
	| { type: 'helm'; namespace?: string }
	| { type: 'sanitizer'; namespace?: string }
	| { type: 'benchmarks' }
	| { type: 'help' };

export interface Suggestion {
	label: string;
	detail: string;
	action: Action;
}

const CONTEXT_COMMANDS = ['ctx', 'context', 'contexts'];
const NAMESPACE_COMMANDS = ['ns', 'namespace', 'namespaces'];
const PORT_FORWARD_COMMANDS = ['pf', 'portforward', 'portforwards'];
const PULSE_COMMANDS = ['pu', 'pulse', 'pulses'];
const XRAY_COMMANDS = ['xray'];
const RBAC_COMMANDS = ['rbac', 'policy', 'subjects'];
const HELM_COMMANDS = ['helm', 'hm', 'chart', 'charts'];
const SANITIZER_COMMANDS = ['popeye', 'pop', 'sanitizer', 'sanitize'];
const BENCH_COMMANDS = ['be', 'bench', 'benchmark', 'benchmarks'];

/** Groups that win when several resources share a name, e.g. core `events` over events.k8s.io. */
const PREFERRED_GROUPS = ['', 'apps', 'batch', 'networking.k8s.io', 'rbac.authorization.k8s.io'];

function groupRank(resource: ResourceInfo) {
	const i = PREFERRED_GROUPS.indexOf(resource.group);
	return i === -1 ? PREFERRED_GROUPS.length : i;
}

function names(resource: ResourceInfo) {
	return [resource.plural, resource.singular, resource.kind.toLowerCase(), ...resource.shortNames];
}

/** How well `term` names `resource`: 0 is an exact alias, higher is weaker, null is no match. */
function score(resource: ResourceInfo, term: string): number | null {
	const all = names(resource);
	const qualified = resource.group ? `${resource.plural}.${resource.group}` : resource.plural;
	if (all.includes(term) || qualified === term) return 0;
	if (all.some((n) => n.startsWith(term))) return 1;
	if (all.some((n) => n.includes(term)) || resource.group.includes(term)) return 2;
	return null;
}

/** Finds resources matching `term`, best first. */
export function matchResources(term: string, resources: ResourceInfo[]): ResourceInfo[] {
	const t = term.toLowerCase();
	return resources
		.map((r) => ({ r, s: score(r, t) }))
		.filter((x): x is { r: ResourceInfo; s: number } => x.s !== null)
		.sort((a, b) => a.s - b.s || groupRank(a.r) - groupRank(b.r) || a.r.kind.localeCompare(b.r.kind))
		.map((x) => x.r);
}

function resourceLabel(r: ResourceInfo) {
	return r.group ? `${r.plural}.${r.group}` : r.plural;
}

/**
 * Turns palette input such as `po`, `deploy kube-system`, `ctx prod` or `ns` into suggestions.
 * The first suggestion is what Enter runs.
 */
export function suggest(
	input: string,
	resources: ResourceInfo[],
	contexts: ContextInfo[],
	namespaces: string[]
): Suggestion[] {
	const [command = '', ...rest] = input.trim().replace(/^:/, '').split(/\s+/);
	const arg = rest.join(' ').toLowerCase();
	const cmd = command.toLowerCase();

	if (cmd === '?' || cmd === 'help') {
		return [{ label: 'help', detail: 'Keyboard shortcuts', action: { type: 'help' } }];
	}

	const portForwards: Suggestion = {
		label: 'pf',
		detail: 'Running port-forwards',
		action: { type: 'portforwards' }
	};
	if (PORT_FORWARD_COMMANDS.includes(cmd)) return [portForwards];

	const pulses = (namespace?: string): Suggestion => ({
		label: namespace ? `pulses ${namespace}` : 'pulses',
		detail: 'Cluster overview: health, warnings, CPU and memory',
		action: { type: 'pulses', namespace }
	});
	if (PULSE_COMMANDS.includes(cmd)) return [pulses(rest[0])];

	const rbac = (namespace?: string): Suggestion => ({
		label: namespace ? `rbac ${namespace}` : 'rbac',
		detail: 'Who may do what: users, groups, service accounts and your own rights',
		action: { type: 'rbac', namespace }
	});
	if (RBAC_COMMANDS.includes(cmd)) return [rbac(rest[0])];

	const helm = (namespace?: string): Suggestion => ({
		label: namespace ? `helm ${namespace}` : 'helm',
		detail: 'Helm releases: values, manifests, history and rollback',
		action: { type: 'helm', namespace }
	});
	if (HELM_COMMANDS.includes(cmd)) return [helm(rest[0])];

	const sanitizer = (namespace?: string): Suggestion => ({
		label: namespace ? `popeye ${namespace}` : 'popeye',
		detail: 'Sanitizer: problems and risky settings, scored like Popeye',
		action: { type: 'sanitizer', namespace }
	});
	if (SANITIZER_COMMANDS.includes(cmd)) return [sanitizer(rest[0])];

	const benchmarks: Suggestion = {
		label: 'bench',
		detail: 'HTTP benchmarks through port-forwards',
		action: { type: 'benchmarks' }
	};
	if (BENCH_COMMANDS.includes(cmd)) return [benchmarks];

	// `xray deploy kube-system`: the kind, then optionally a namespace.
	const xray = (kind: string, namespace?: string) => {
		const roots = xrayResources(resources);
		return (kind ? matchResources(kind, roots) : roots).map(
			(r): Suggestion => ({
				label: `xray ${resourceLabel(r)}${namespace ? ` ${namespace}` : ''}`,
				detail: `How ${r.kind}s depend on pods, config and volumes`,
				action: { type: 'xray', resource: r, namespace }
			})
		);
	};
	if (XRAY_COMMANDS.includes(cmd)) return xray(rest[0] ?? '', rest[1]);

	if (CONTEXT_COMMANDS.includes(cmd)) {
		return contexts
			.filter((c) => c.name.toLowerCase().includes(arg))
			.map((c) => ({
				label: `ctx ${c.name}`,
				detail: [c.cluster, c.namespace && `ns ${c.namespace}`].filter(Boolean).join(' · '),
				action: { type: 'context', name: c.name }
			}));
	}

	if (NAMESPACE_COMMANDS.includes(cmd) && (rest.length > 0 || input.endsWith(' '))) {
		const all: Suggestion = { label: 'ns all', detail: 'All namespaces', action: { type: 'namespace', name: '' } };
		const matches = namespaces
			.filter((n) => n.toLowerCase().includes(arg))
			.map((n) => ({ label: `ns ${n}`, detail: 'Namespace', action: { type: 'namespace', name: n } }) as Suggestion);
		return 'all'.startsWith(arg) ? [all, ...matches] : matches;
	}

	const resourceSuggestions = matchResources(cmd, resources)
		.slice(0, 30)
		.map(
			(r): Suggestion => ({
				label: rest.length ? `${resourceLabel(r)} ${rest.join(' ')}` : resourceLabel(r),
				detail: [r.kind, r.shortNames.length ? r.shortNames.join(', ') : '', r.namespaced ? '' : 'cluster']
					.filter(Boolean)
					.join(' · '),
				action: { type: 'resource', resource: r, namespace: rest.length ? rest[0] : undefined }
			})
		);

	// Offer the context and namespace commands while they are being typed.
	const extra: Suggestion[] = [];
	if (cmd && 'ctx'.startsWith(cmd) && contexts.length) {
		extra.push(...suggest('ctx', resources, contexts, namespaces).slice(0, 5));
	}
	if (cmd && 'ns'.startsWith(cmd)) {
		extra.push(...suggest('ns ', resources, contexts, namespaces).slice(0, 5));
	}
	if (cmd && PORT_FORWARD_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(portForwards);
	if (cmd && PULSE_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(pulses());
	if (cmd && 'xray'.startsWith(cmd)) extra.push(...xray('').slice(0, 3));
	if (cmd && RBAC_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(rbac());
	if (cmd && HELM_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(helm());
	if (cmd && SANITIZER_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(sanitizer());
	if (cmd && BENCH_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(benchmarks);
	return [...resourceSuggestions, ...extra];
}

import type { Alias, ContextInfo, ResourceInfo } from './api';
import { CORE_GROUPS, parseShortForm, TEMPLATE_KINDS, type TemplateParams } from './create';
import { xrayResources } from './xray';

export type ConfigSection = 'contexts' | 'diffs' | 'agents' | 'explain' | 'skins' | 'hotkeys' | 'plugins' | 'aliases';

export type Action =
	| {
			type: 'resource';
			resource: ResourceInfo;
			namespace?: string;
			/** Shown filtered, such as `-l app=web`. */
			filter?: string;
			/** Opened in another context, like k9s `:pods @prod`. */
			context?: string;
	  }
	| { type: 'context'; name: string }
	| { type: 'namespace'; name: string }
	| { type: 'portforwards' }
	| { type: 'pulses'; namespace?: string }
	| { type: 'xray'; resource: ResourceInfo; namespace?: string }
	| { type: 'rbac'; namespace?: string }
	| { type: 'helm'; namespace?: string }
	| { type: 'new'; resource?: ResourceInfo; params?: TemplateParams }
	| { type: 'sanitizer'; namespace?: string }
	| { type: 'benchmarks' }
	| { type: 'config'; section?: ConfigSection }
	| { type: 'skin'; name: string | null }
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
const NEW_COMMANDS = ['new', 'create', 'apply'];
const SANITIZER_COMMANDS = ['popeye', 'pop', 'sanitizer', 'sanitize'];
const BENCH_COMMANDS = ['be', 'bench', 'benchmark', 'benchmarks'];
const CONFIG_COMMANDS: Record<string, ConfigSection | undefined> = {
	config: undefined,
	settings: undefined,
	k9s: undefined,
	agents: 'agents',
	mcp: 'agents',
	explain: 'explain',
	ai: 'explain',
	skins: 'skins',
	hotkeys: 'hotkeys',
	hotkey: 'hotkeys',
	plugins: 'plugins',
	plugin: 'plugins',
	aliases: 'aliases',
	alias: 'aliases',
	a: 'aliases'
};
const SKIN_COMMANDS = ['skin'];

/** The k9s configuration the palette uses. */
export interface PaletteConfig {
	aliases: Alias[];
	skins: string[];
}

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
 * The resource of `v1/pods` or `apps/v1/deployments`, as k9s aliases name them. The version is
 * ignored if the cluster serves another one.
 */
export function resourceByPath(path: string, resources: ResourceInfo[]): ResourceInfo | undefined {
	const parts = path.toLowerCase().split('/');
	if (parts.length < 2 || parts.length > 3 || parts.some((p) => !p)) return undefined;
	const [group, version, plural] = parts.length === 2 ? ['', ...parts] : parts;
	return (
		resources.find((r) => r.group === group && r.version === version && r.plural === plural) ??
		resources.find((r) => r.group === group && r.plural === plural)
	);
}

type ResourceArgs = Pick<Extract<Action, { type: 'resource' }>, 'namespace' | 'filter' | 'context'>;

/** What follows a resource name: a namespace, `key=value` label filters, `-l` and `@context`. */
export function resourceArgs(args: string[]): ResourceArgs {
	let namespace: string | undefined;
	let context: string | undefined;
	const labels: string[] = [];
	for (let i = 0; i < args.length; i++) {
		const arg = args[i];
		if (arg === '-l' && i + 1 < args.length) labels.push(args[++i]);
		else if (arg.startsWith('@') && arg.length > 1) context = arg.slice(1);
		else if (arg.includes('=')) labels.push(arg);
		else namespace ??= arg;
	}
	const out: ResourceArgs = {};
	if (namespace !== undefined) out.namespace = namespace;
	if (labels.length) out.filter = `-l ${labels.join(',')}`;
	if (context !== undefined) out.context = context;
	return out;
}

/**
 * Turns palette input such as `po`, `deploy kube-system`, `ctx prod` or `ns` into suggestions.
 * The first suggestion is what Enter runs.
 */
export function suggest(
	input: string,
	resources: ResourceInfo[],
	contexts: ContextInfo[],
	namespaces: string[],
	config: PaletteConfig = { aliases: [], skins: [] },
	depth = 0
): Suggestion[] {
	const [command = '', ...rest] = input.trim().replace(/^:/, '').split(/\s+/);
	// A k9s alias stands for another command, which may itself be an alias.
	const alias = config.aliases.find((a) => a.name.toLowerCase() === command.toLowerCase());
	if (alias && depth < 5) {
		const expanded = [alias.command, ...rest].join(' ');
		const target = suggest(expanded, resources, contexts, namespaces, config, depth + 1)[0];
		if (target) {
			const label = [command, ...rest].join(' ');
			const own: Suggestion = { label, detail: `${alias.command} · alias`, action: target.action };
			return depth ? [own] : [own, ...builtins(input, resources, contexts, namespaces, config)];
		}
	}
	return builtins(input, resources, contexts, namespaces, config);
}

function builtins(
	input: string,
	resources: ResourceInfo[],
	contexts: ContextInfo[],
	namespaces: string[],
	config: PaletteConfig
): Suggestion[] {
	const [command = '', ...rest] = input.trim().replace(/^:/, '').split(/\s+/);
	const arg = rest.join(' ').toLowerCase();
	const cmd = command.toLowerCase();

	// `v1/pods` or `apps/v1/deployments kube-system`.
	if (cmd.includes('/')) {
		const r = resourceByPath(cmd, resources);
		if (!r) return [];
		return [
			{
				label: [command, ...rest].join(' '),
				detail: r.kind,
				action: { type: 'resource', resource: r, ...resourceArgs(rest) }
			}
		];
	}

	// First, but resources stay on offer: `a` is also how `apiservices` starts.
	const configPage: Suggestion[] = [];
	if (Object.hasOwn(CONFIG_COMMANDS, cmd)) {
		const section = CONFIG_COMMANDS[cmd];
		configPage.push({
			label: cmd,
			detail: 'Settings: contexts, and the k9s skins, hotkeys, plugins and aliases',
			action: section ? { type: 'config', section } : { type: 'config' }
		});
	}

	if (SKIN_COMMANDS.includes(cmd)) {
		const reset: Suggestion = { label: 'skin kubri', detail: "Kubri's own colors", action: { type: 'skin', name: null } };
		const skins = config.skins
			.filter((name) => name.toLowerCase().includes(arg))
			.map((name): Suggestion => ({ label: `skin ${name}`, detail: 'k9s skin', action: { type: 'skin', name } }));
		return 'kubri'.startsWith(arg) ? [...skins, reset] : skins;
	}

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

	const blank: Suggestion = {
		label: 'new',
		detail: 'Create objects from a template, a file, a URL or the clipboard',
		action: { type: 'new' }
	};
	// `new deploy web --image nginx --replicas 3`, like kubectl create.
	if (NEW_COMMANDS.includes(cmd)) {
		const form = parseShortForm(input.trim().replace(/^:/, '').slice(command.length));
		const creatable = resources.filter((r) => r.verbs.includes('create') && !r.plural.includes('/'));
		if (!form) {
			const common = TEMPLATE_KINDS.flatMap((k) => creatable.filter((r) => r.kind === k && CORE_GROUPS.includes(r.group)).slice(0, 1));
			return [
				{ ...blank, label: cmd },
				...common.map((r): Suggestion => ({ label: `${cmd} ${r.singular || r.kind.toLowerCase()}`, detail: `New ${r.kind}`, action: { type: 'new', resource: r } }))
			];
		}
		const more = rest.slice(1).join(' ');
		return matchResources(form.kind, creatable)
			.slice(0, 8)
			.map((r): Suggestion => ({
				label: [cmd, r.singular || r.kind.toLowerCase(), more].filter(Boolean).join(' '),
				detail: `New ${r.kind}${form.params.name ? ` ${form.params.name}` : ''}${r.group ? ` · ${r.group}` : ''}`,
				action: { type: 'new', resource: r, params: form.params }
			}));
	}

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
				action: { type: 'resource', resource: r, ...resourceArgs(rest) }
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
	if (cmd && NEW_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(blank);
	if (cmd && SANITIZER_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(sanitizer());
	if (cmd && BENCH_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(benchmarks);
	if (cmd.length > 1 && 'config'.startsWith(cmd) && cmd !== 'config') {
		extra.push(...builtins('config', resources, contexts, namespaces, config).slice(0, 1));
	}
	if (cmd.length > 1 && 'skin'.startsWith(cmd)) extra.push(...builtins('skin', resources, contexts, namespaces, config).slice(0, 5));
	// Aliases while they are being typed, if they lead somewhere.
	const aliases = cmd
		? config.aliases
				.filter((a) => a.name.toLowerCase().startsWith(cmd) && a.name.toLowerCase() !== cmd)
				.slice(0, 5)
				.flatMap((a) => suggest(a.name, resources, contexts, namespaces, config, 1).filter((s) => s.label === a.name))
		: [];
	return [...configPage, ...resourceSuggestions, ...aliases, ...extra];
}

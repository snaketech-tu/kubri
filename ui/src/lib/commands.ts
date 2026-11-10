import type { Alias, ContextInfo, ObjectRef, ResourceInfo } from './api';
import { parseChangesArgs, type ChangesFilter } from './changes';
import { CORE_GROUPS, parseShortForm, TEMPLATE_KINDS, type TemplateParams } from './create';
import { xrayResources } from './xray';
import { t } from './i18n';

export type ConfigSection = 'contexts' | 'diffs' | 'changes' | 'agents' | 'explain' | 'appearance' | 'updates' | 'views' | 'skins' | 'hotkeys' | 'plugins' | 'aliases';

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
	| { type: 'tools' }
	| { type: 'kinds'; filter?: string }
	| { type: 'pulses'; namespace?: string }
	| { type: 'xray'; resource: ResourceInfo; namespace?: string }
	| { type: 'rbac'; namespace?: string }
	| { type: 'helm'; namespace?: string }
	| { type: 'new'; resource?: ResourceInfo; params?: TemplateParams }
	| { type: 'sanitizer'; namespace?: string }
	| { type: 'benchmarks' }
	| { type: 'integrations' }
	| { type: 'rightsizing' }
	| { type: 'fleet' }
	| { type: 'terminal'; another?: boolean }
	| { type: 'search'; query?: string }
	| { type: 'graph'; namespace?: string }
	| { type: 'changes'; filter?: ChangesFilter }
	/** The snapshot dialog; `namespace` null for the whole cluster, unset for the current namespace. */
	| { type: 'snapshot'; namespace?: string | null }
	/** The kubeconfig files Kubri reads, where more can be loaded. */
	| { type: 'kubeconfig' }
	/** One object, in whichever context it is. */
	| { type: 'object'; target: ObjectRef; kind?: string }
	| { type: 'config'; section?: ConfigSection }
	| { type: 'skin'; name: string | null }
	| { type: 'help' }
	/** A new GitHub issue: a problem or an idea, with the version filled in. */
	| { type: 'feedback'; kind: 'bug' | 'idea' }
	/** Runs something of the app itself, from the palette. */
	| { type: 'call'; run: () => void };

export interface Suggestion {
	label: string;
	detail: string;
	action: Action;
	/** Why it cannot run, such as a namespace that does not exist; the palette stays open. */
	refused?: boolean;
}

const feedback: Suggestion[] = [
	{ label: 'feedback', detail: t('Report a problem on GitHub, with your Kubri version filled in'), action: { type: 'feedback', kind: 'bug' } },
	{ label: 'feedback idea', detail: t('Suggest an idea on GitHub'), action: { type: 'feedback', kind: 'idea' } }
];

const CONTEXT_COMMANDS = ['ctx', 'context', 'contexts'];
const NAMESPACE_COMMANDS = ['ns', 'namespace', 'namespaces'];
const PORT_FORWARD_COMMANDS = ['pf', 'portforward', 'portforwards'];
const PULSE_COMMANDS = ['overview', 'pu', 'pulse', 'pulses'];
const XRAY_COMMANDS = ['xray'];
const RBAC_COMMANDS = ['access', 'rbac', 'policy', 'subjects'];
const HELM_COMMANDS = ['helm', 'hm', 'chart', 'charts'];
const NEW_COMMANDS = ['new', 'create', 'apply'];
const SANITIZER_COMMANDS = ['checks', 'check', 'popeye', 'pop', 'sanitizer', 'sanitize'];
const TOOLS_COMMANDS = ['tools', 'tool'];
const KINDS_COMMANDS = ['kinds', 'api-resources'];
const BENCH_COMMANDS = ['be', 'bench', 'benchmark', 'benchmarks'];
const INTEGRATION_COMMANDS = ['integrations', 'integration', 'prometheus', 'grafana', 'loki'];
const SIZING_COMMANDS = ['rightsizing', 'rightsize', 'sizing'];
const FLEET_COMMANDS = ['fleet', 'home'];
const TERMINAL_COMMANDS = ['term', 'terminal'];
const SEARCH_COMMANDS = ['search', 'find', 'grep'];
const GRAPH_COMMANDS = ['graph', 'topology'];
const CHANGES_COMMANDS = ['changes', 'changed', 'history', 'timeline'];
const SNAPSHOT_COMMANDS = ['snapshot', 'snap', 'snapshots'];
const KUBECONFIG_COMMANDS = ['kubeconfig', 'kubeconfigs'];
const CONFIG_COMMANDS: Record<string, ConfigSection | undefined> = {
	config: undefined,
	settings: undefined,
	k9s: undefined,
	agents: 'agents',
	mcp: 'agents',
	explain: 'explain',
	ai: 'explain',
	views: 'views',
	appearance: 'appearance',
	theme: 'appearance',
	zoom: 'appearance',
	updates: 'updates',
	update: 'updates',
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
	/** Whether a local terminal can be opened here; the web app needs the server's leave. */
	terminal?: boolean;
}

/** Namespaces that win when several start the same way: `kube` is kube-system. */
const PREFERRED_NAMESPACES = ['default', 'kube-system'];

/**
 * The names `typed` may stand for, best first: itself, then those it starts, then those it is
 * part of, ignoring case.
 */
export function completions(typed: string, names: string[], preferred: string[] = []): string[] {
	const lower = typed.toLowerCase();
	const exact = names.filter((n) => n.toLowerCase() === lower);
	const rank = (n: string) => {
		const i = preferred.indexOf(n);
		return i === -1 ? preferred.length : i;
	};
	const order = (a: string, b: string) => rank(a) - rank(b) || a.length - b.length || a.localeCompare(b);
	const starting = names.filter((n) => n.toLowerCase() !== lower && n.toLowerCase().startsWith(lower)).sort(order);
	const inside = names.filter((n) => !n.toLowerCase().startsWith(lower) && n.toLowerCase().includes(lower)).sort(order);
	return [...exact, ...starting, ...inside];
}

/**
 * The namespaces a typed namespace argument may stand for, best first. As typed when Kubri
 * knows no namespaces or for `all`; empty when none fits.
 */
export function namespaceCompletions(typed: string, namespaces: string[]): string[] {
	if (!namespaces.length || typed.toLowerCase() === 'all') return [typed];
	return completions(typed, namespaces, PREFERRED_NAMESPACES);
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
	// Two letters inside a word are chance, like `dp` in endpoints or `rb` in rbac.authorization.k8s.io.
	if (term.length < 3) return null;
	if (all.some((n) => n.includes(term)) || resource.group.includes(term)) return 2;
	return null;
}

/** Finds resources matching `term`, best first. */
export function matchResources(term: string, resources: ResourceInfo[]): ResourceInfo[] {
	const lower = term.toLowerCase();
	return resources
		.map((r) => ({ r, s: score(r, lower) }))
		.filter((x): x is { r: ResourceInfo; s: number } => x.s !== null)
		.sort((a, b) => a.s - b.s || groupRank(a.r) - groupRank(b.r) || a.r.kind.localeCompare(b.r.kind))
		.map((x) => x.r);
}

/**
 * Whether `term` names the action `label` by its verb, as `restart` names "Restart" and
 * `scale` names "Scale…". A kind keeps its name: `events` stays the list of events.
 */
export function namesAction(label: string, term: string, resources: ResourceInfo[]): boolean {
	const word = term.trim().toLowerCase();
	if (!word || /\s/.test(word)) return false;
	if (resources.some((r) => names(r).includes(word))) return false;
	return label.toLowerCase().replace(/…$/, '').split(/\s+/)[0] === word;
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
			const own: Suggestion = target.refused
				? { ...target, label }
				: { label, detail: t('{command} · alias', { command: alias.command }), action: target.action };
			return depth ? [own] : [own, ...builtins(input, resources, contexts, namespaces, config)];
		}
	}
	return builtins(input, resources, contexts, namespaces, config);
}

/** "No namespace sh", as a suggestion that does not run. */
function noNamespace(label: string, typed: string, action: Action): Suggestion {
	return { label, detail: t('No namespace {name}', { name: typed }), action, refused: true };
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
	const contextNames = contexts.map((c) => c.name);

	/**
	 * A command that takes a namespace as its first argument, such as `helm sh`: the namespace
	 * completed, with the other namespaces it may mean after it.
	 */
	const withNamespace = (make: (namespace?: string) => Suggestion, typed: string | undefined, more = 0): Suggestion[] => {
		if (!typed) return [make()];
		const found = namespaceCompletions(typed, namespaces);
		if (!found.length) return [noNamespace(make(typed).label, typed, make(typed).action)];
		return found.slice(0, 1 + more).map((ns) => make(ns));
	};

	/**
	 * What follows a resource name, with its namespace and `@context` completed: one set of
	 * arguments per namespace it may mean, best first, or null when no namespace fits.
	 */
	const completedArgs = (r: ResourceInfo, args: string[], more: number): { args: string[]; action: ResourceArgs }[] | null => {
		const parsed = resourceArgs(args);
		let words = args;
		if (parsed.context !== undefined) {
			const context = completions(parsed.context, contextNames)[0];
			if (context && context !== parsed.context) words = words.map((w) => (w === `@${parsed.context}` ? `@${context}` : w));
		}
		const typed = r.namespaced ? parsed.namespace : undefined;
		// Another context's namespaces are not known here.
		if (typed === undefined || resourceArgs(words).context !== undefined) return [{ args: words, action: resourceArgs(words) }];
		const found = namespaceCompletions(typed, namespaces);
		if (!found.length) return null;
		const at = words.indexOf(typed);
		return found.slice(0, 1 + more).map((ns) => {
			const these = words.map((w, i) => (i === at ? ns : w));
			return { args: these, action: resourceArgs(these) };
		});
	};

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
			detail: t('Settings: contexts, and the k9s skins, hotkeys, plugins and aliases'),
			action: section ? { type: 'config', section } : { type: 'config' }
		});
	}

	if (SKIN_COMMANDS.includes(cmd)) {
		const reset: Suggestion = { label: 'skin kubri', detail: t("Kubri's own colors"), action: { type: 'skin', name: null } };
		const skins = config.skins
			.filter((name) => name.toLowerCase().includes(arg))
			.map((name): Suggestion => ({ label: `skin ${name}`, detail: t('k9s skin'), action: { type: 'skin', name } }));
		return 'kubri'.startsWith(arg) ? [...skins, reset] : skins;
	}

	if (cmd === '?' || cmd === 'help') {
		return [{ label: 'help', detail: t('Keyboard shortcuts'), action: { type: 'help' } }];
	}

	if (cmd === 'feedback') return arg && 'idea'.startsWith(arg) ? [feedback[1], feedback[0]] : feedback;

	const portForwards: Suggestion = {
		label: 'pf',
		detail: t('Running port-forwards'),
		action: { type: 'portforwards' }
	};
	if (PORT_FORWARD_COMMANDS.includes(cmd)) return [portForwards];

	// The views go by one name; the k9s commands still lead there.
	const pulses = (namespace?: string): Suggestion => ({
		label: namespace ? `overview ${namespace}` : 'overview',
		detail: t('Overview: health, warnings, CPU and memory · also :pulses'),
		action: { type: 'pulses', namespace }
	});
	if (PULSE_COMMANDS.includes(cmd)) return withNamespace(pulses, rest[0], 4);

	const rbac = (namespace?: string): Suggestion => ({
		label: namespace ? `access ${namespace}` : 'access',
		detail: t('Access: who may do what, and your own rights · also :rbac'),
		action: { type: 'rbac', namespace }
	});
	if (RBAC_COMMANDS.includes(cmd)) return withNamespace(rbac, rest[0], 4);

	const helm = (namespace?: string): Suggestion => ({
		label: namespace ? `helm ${namespace}` : 'helm',
		detail: t('Helm releases: values, manifests, history and rollback'),
		action: { type: 'helm', namespace }
	});
	if (HELM_COMMANDS.includes(cmd)) return withNamespace(helm, rest[0], 4);

	const sanitizer = (namespace?: string): Suggestion => ({
		label: namespace ? `checks ${namespace}` : 'checks',
		detail: t('Checks: common mistakes and risky settings, scored · also :popeye'),
		action: { type: 'sanitizer', namespace }
	});
	if (SANITIZER_COMMANDS.includes(cmd)) return withNamespace(sanitizer, rest[0], 4);

	const benchmarks: Suggestion = {
		label: 'bench',
		detail: t('HTTP benchmarks through port-forwards'),
		action: { type: 'benchmarks' }
	};
	if (BENCH_COMMANDS.includes(cmd)) return [benchmarks];

	const integrations: Suggestion = {
		label: 'integrations',
		detail: t('Prometheus, Grafana, Loki, Flux, Argo CD and cert-manager in this cluster'),
		action: { type: 'integrations' }
	};
	if (INTEGRATION_COMMANDS.includes(cmd)) return [integrations];

	const sizing: Suggestion = {
		label: 'rightsizing',
		detail: t('Containers that request much more or less than they use'),
		action: { type: 'rightsizing' }
	};
	if (SIZING_COMMANDS.includes(cmd)) return [sizing];

	const tools: Suggestion = { label: 'tools', detail: t('Every tool beyond the lists, with its command'), action: { type: 'tools' } };
	if (TOOLS_COMMANDS.includes(cmd)) return [tools];

	const kinds: Suggestion = {
		label: arg ? `kinds ${arg}` : 'kinds',
		detail: t('Every kind the cluster serves, to find one and open its list'),
		action: { type: 'kinds', filter: arg || undefined }
	};
	if (KINDS_COMMANDS.includes(cmd)) return [kinds];

	const fleet: Suggestion = {
		label: 'fleet',
		detail: t('Every context at once: reachable, version and health'),
		action: { type: 'fleet' }
	};
	if (FLEET_COMMANDS.includes(cmd)) return [fleet];

	const terminal = config.terminal !== false;
	const terminals: Suggestion[] = [
		{ label: 'term', detail: t('A terminal here with kubectl and helm set to this context and namespace'), action: { type: 'terminal' } },
		{ label: 'term new', detail: t('Another terminal, also when one is open'), action: { type: 'terminal', another: true } }
	];
	if (terminal && TERMINAL_COMMANDS.includes(cmd)) return arg === 'new' ? [terminals[1]] : terminals;

	// `search web kind:deploy ctx:*`: everything after the command is the query.
	const query = input.trim().replace(/^:/, '').slice(command.length).trim();
	const search: Suggestion = {
		label: query ? `${cmd} ${query}` : 'search',
		detail: query ? t('Search objects by name, label and field') : t('Objects by name, label and field, in one or more contexts'),
		action: { type: 'search', query }
	};
	if (SEARCH_COMMANDS.includes(cmd)) return [search];

	const graph = (namespace?: string): Suggestion => ({
		label: namespace ? `graph ${namespace}` : 'graph',
		detail: t('Topology as one graph: how objects connect, and the request paths of Ingresses'),
		action: { type: 'graph', namespace }
	});
	if (GRAPH_COMMANDS.includes(cmd)) return withNamespace(graph, rest[0], 4);

	// `changes shop 6h`: a namespace and how far back, both optional.
	const changes = (filter: ChangesFilter = {}): Suggestion => {
		const hours = filter.hours ? (filter.hours < 1 ? `${Math.round(filter.hours * 60)}m` : `${filter.hours}h`) : '';
		return {
			label: ['changes', filter.namespace, hours].filter(Boolean).join(' '),
			detail: t('What changed: rollouts, scaling, config and warnings Kubri recorded'),
			action: { type: 'changes', filter }
		};
	};
	if (CHANGES_COMMANDS.includes(cmd)) {
		const filter = parseChangesArgs(rest);
		if (filter.namespace) {
			const typed = filter.namespace;
			const found = withNamespace((namespace) => changes({ ...filter, namespace }), typed);
			if (found[0].refused) return found;
			filter.namespace = (found[0].action as { filter: ChangesFilter }).filter.namespace;
		}
		return [changes(filter), ...(filter.hours ? [] : [changes({ ...filter, hours: 1 })])];
	}

	// `snapshot shop` or `snapshot all`: what to save, the current namespace without one.
	const snapshot = (arg?: string): Suggestion => ({
		label: arg ? `snapshot ${arg}` : 'snapshot',
		detail:
			arg === 'all'
				? t('Save the whole cluster to a file to open offline, or open one')
				: arg
					? t('Save namespace {name} to a file to open offline, or open one', { name: arg })
					: t('Save this namespace to a file to open offline, or open one'),
		action: { type: 'snapshot', namespace: arg === 'all' ? null : arg }
	});
	if (SNAPSHOT_COMMANDS.includes(cmd)) return rest[0] ? withNamespace(snapshot, rest[0]) : [snapshot(), snapshot('all')];
	if (KUBECONFIG_COMMANDS.includes(cmd)) {
		return [{ label: 'kubeconfig', detail: t('The kubeconfig files Kubri reads, and load more'), action: { type: 'kubeconfig' } }];
	}

	// `xray deploy kube-system`: the kind, then optionally a namespace.
	const xray = (kind: string, namespace?: string) => {
		const roots = xrayResources(resources);
		return (kind ? matchResources(kind, roots) : roots).map(
			(r): Suggestion => ({
				label: `xray ${resourceLabel(r)}${namespace ? ` ${namespace}` : ''}`,
				detail: t('Topology of {kinds}: how they depend on pods, config and volumes', { kinds: `${r.kind}s` }),
				action: { type: 'xray', resource: r, namespace }
			})
		);
	};
	if (XRAY_COMMANDS.includes(cmd)) {
		const typed = rest[1];
		const found = typed ? namespaceCompletions(typed, namespaces) : [undefined];
		const shown = xray(rest[0] ?? '', found[0] ?? typed);
		return typed && !found.length ? shown.slice(0, 1).map((s) => noNamespace(s.label, typed, s.action)) : shown;
	}

	const blank: Suggestion = {
		label: 'new',
		detail: t('Create objects from a template, a file, a URL or the clipboard'),
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
				...common.map((r): Suggestion => ({ label: `${cmd} ${r.singular || r.kind.toLowerCase()}`, detail: t('New {kind}', { kind: r.kind }), action: { type: 'new', resource: r } }))
			];
		}
		const more = rest.slice(1).join(' ');
		return matchResources(form.kind, creatable)
			.slice(0, 8)
			.map((r): Suggestion => ({
				label: [cmd, r.singular || r.kind.toLowerCase(), more].filter(Boolean).join(' '),
				detail: `${t('New {kind}', { kind: r.kind })}${form.params.name ? ` ${form.params.name}` : ''}${r.group ? ` · ${r.group}` : ''}`,
				action: { type: 'new', resource: r, params: form.params }
			}));
	}

	if (CONTEXT_COMMANDS.includes(cmd)) {
		if (arg === '-') return [{ label: 'ctx -', detail: t('The previous context'), action: { type: 'context', name: '-' } }];
		const order = completions(arg, contextNames);
		const matches = (arg ? order.map((name) => contexts.find((c) => c.name === name)!) : contexts).map(
			(c): Suggestion => ({
				label: `ctx ${c.name}`,
				detail: [c.cluster, c.namespace && `ns ${c.namespace}`].filter(Boolean).join(' · '),
				action: { type: 'context', name: c.name }
			})
		);
		// Like k9s, a bare `:ctx` lists the contexts rather than switching to the first one.
		if (!arg && !input.endsWith(' ')) return [{ ...fleet, label: cmd, detail: t('Every context in your kubeconfig, to pick one') }, ...matches];
		return matches;
	}

	if (NAMESPACE_COMMANDS.includes(cmd) && (rest.length > 0 || input.endsWith(' '))) {
		const all: Suggestion = { label: 'ns all', detail: t('All namespaces'), action: { type: 'namespace', name: '' } };
		const matches = (arg ? completions(arg, namespaces, PREFERRED_NAMESPACES) : namespaces)
			.map((n) => ({ label: `ns ${n}`, detail: t('Namespace'), action: { type: 'namespace', name: n } }) as Suggestion);
		return 'all'.startsWith(arg) ? [all, ...matches] : matches;
	}

	// `po sh` is the pods of shop: the namespace is completed, and the best kind also offers the
	// other namespaces it may mean.
	const resourceSuggestions = matchResources(cmd, resources)
		.slice(0, 30)
		.flatMap((r, i): Suggestion[] => {
			const detail = [r.kind, r.shortNames.length ? r.shortNames.join(', ') : '', r.namespaced ? '' : t('cluster-wide')].filter(Boolean).join(' · ');
			const label = (args: string[]) => (args.length ? `${resourceLabel(r)} ${args.join(' ')}` : resourceLabel(r));
			const completed = completedArgs(r, rest, i === 0 ? 4 : 0);
			if (!completed) return [noNamespace(label(rest), resourceArgs(rest).namespace ?? '', { type: 'resource', resource: r, ...resourceArgs(rest) })];
			return completed.map(({ args, action }) => ({ label: label(args), detail, action: { type: 'resource', resource: r, ...action } }));
		});

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
	if (cmd.length > 1 && INTEGRATION_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(integrations);
	if (cmd.length > 2 && SIZING_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(sizing);
	if (cmd.length > 2 && TOOLS_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(tools);
	if (cmd.length > 1 && KINDS_COMMANDS.some((c) => c.startsWith(cmd))) extra.push({ ...kinds, label: 'kinds' });
	if (cmd.length > 1 && FLEET_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(fleet);
	if (terminal && cmd.length > 1 && TERMINAL_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(terminals[0]);
	if (cmd.length > 1 && SEARCH_COMMANDS.some((c) => c.startsWith(cmd))) extra.push({ ...search, label: 'search' });
	if (cmd.length > 1 && GRAPH_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(graph());
	if (cmd.length > 1 && CHANGES_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(changes(), changes({ hours: 1 }));
	if (cmd.length > 2 && SNAPSHOT_COMMANDS.some((c) => c.startsWith(cmd))) extra.push(snapshot());
	if (cmd.length > 2 && 'feedback'.startsWith(cmd) && cmd !== 'feedback') extra.push(...feedback);
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

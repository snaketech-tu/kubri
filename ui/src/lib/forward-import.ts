/**
 * Port-forward favorites in bulk: read from pasted `kubectl port-forward` commands or a favorites
 * file, made from chosen objects, and given free local ports before they are kept.
 */
import { parse, stringify } from 'yaml';
import type { FavoriteForward, KubeObject, ResourceInfo, ResourceRef } from './api';
import { t } from './i18n';
import { annotatedForwards, forwardablePorts, PORT_FORWARDS_KEY } from './pods';

/** The kinds a favorite can forward to, by their plural. */
export type ForwardKind = 'pods' | 'services' | 'deployments' | 'statefulsets' | 'daemonsets' | 'replicasets';

/** How kubectl and the favorites file may name a kind, singular, plural or short. */
const KIND_NAMES: Record<string, ForwardKind> = {
	po: 'pods',
	pod: 'pods',
	pods: 'pods',
	svc: 'services',
	service: 'services',
	services: 'services',
	deploy: 'deployments',
	deployment: 'deployments',
	deployments: 'deployments',
	sts: 'statefulsets',
	statefulset: 'statefulsets',
	statefulsets: 'statefulsets',
	ds: 'daemonsets',
	daemonset: 'daemonsets',
	daemonsets: 'daemonsets',
	rs: 'replicasets',
	replicaset: 'replicasets',
	replicasets: 'replicasets'
};

const GROUPS: Record<ForwardKind, string> = {
	pods: '',
	services: '',
	deployments: 'apps',
	statefulsets: 'apps',
	daemonsets: 'apps',
	replicasets: 'apps'
};

/** The kind a name like `svc`, `deployment` or `deployments.apps` means, if a favorite can forward to it. */
export function forwardKind(name: string): ForwardKind | null {
	const [plain, ...group] = name.toLowerCase().split('.');
	const kind = KIND_NAMES[plain];
	if (!kind) return null;
	// `deployments.apps` and `deployments.v1.apps` name the group too; it must be the right one.
	if (group.length && group.at(-1) !== GROUPS[kind] && !(GROUPS[kind] === '' && group.at(-1) === 'v1')) return null;
	return kind;
}

/** The cluster's resource for a kind, if it serves it. */
export function resourceOf(resources: ResourceInfo[], kind: ForwardKind): ResourceInfo | undefined {
	return resources.find((r) => r.group === GROUPS[kind] && r.plural === kind);
}

/** A favorite as read or made, before it is checked against the cluster and given its local port. */
export interface ForwardEntry {
	kind: ForwardKind;
	/** Null where none was given: the context's namespace. */
	namespace: string | null;
	name: string;
	/** A port number, or a port name to look up on the object. */
	remote: number | string;
	/** Null for Kubri to choose a free port. */
	local: number | null;
	/** Where to listen; empty for 127.0.0.1. */
	address: string;
	/** What to call the favorite; empty for its target. */
	label: string;
	/** Null where nothing says: the choice in the dialog. */
	startOnConnect: boolean | null;
	/** The kubectl context a command named, to point out one for another cluster. */
	context: string | null;
}

/** A line or entry that could not be read, and why. */
export interface ImportProblem {
	/** Where it is: a line number for commands, an entry number for a file. */
	at: number;
	text: string;
	reason: string;
}

export interface ImportResult {
	entries: ForwardEntry[];
	problems: ImportProblem[];
	/** What the text was read as. */
	format: 'kubectl' | 'file' | 'none';
}

function entry(kind: ForwardKind, name: string, remote: number | string, more: Partial<ForwardEntry> = {}): ForwardEntry {
	return { kind, namespace: null, name, remote, local: null, address: '', label: '', startOnConnect: null, context: null, ...more };
}

/** A port as typed: a number in range, or a port name. Null when it is neither. */
function portValue(text: string): number | string | null {
	if (/^\d+$/.test(text)) {
		const n = Number(text);
		return n >= 1 && n <= 65535 ? n : null;
	}
	return /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/i.test(text) && text.length <= 15 ? text : null;
}

/** A local port typed as a number, 0 for one Kubri chooses. Undefined when it is not a port. */
function localValue(value: unknown): number | null | undefined {
	if (value === undefined || value === null || value === '' || value === 0 || value === '0') return null;
	const n = typeof value === 'number' ? value : typeof value === 'string' && /^\d+$/.test(value.trim()) ? Number(value) : NaN;
	return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : undefined;
}

/**
 * Reads favorites from text: a favorites file (YAML or JSON) as Kubri exports it, or else any
 * lines with `kubectl port-forward` commands, as in a script. Lines without one are skipped.
 */
export function parseImport(text: string): ImportResult {
	if (!text.trim()) return { entries: [], problems: [], format: 'none' };
	const file = readFile(text);
	if (file) return { ...file, format: 'file' };
	const commands = parseCommands(text);
	return { ...commands, format: commands.entries.length || commands.problems.length ? 'kubectl' : 'none' };
}

// kubectl options that take a value, so the value is not taken for the target or a port.
const VALUE_FLAGS = new Set([
	'-n',
	'--namespace',
	'--context',
	'--address',
	'--kubeconfig',
	'--cluster',
	'--user',
	'-s',
	'--server',
	'--token',
	'--as',
	'--as-group',
	'--as-uid',
	'--request-timeout',
	'--pod-running-timeout',
	'--certificate-authority',
	'--client-certificate',
	'--client-key',
	'--tls-server-name',
	'--cache-dir',
	'-v',
	'--v',
	'--profile',
	'--profile-output'
]);

/** Splits a command line into words, as a shell would for plain quoting. */
function words(line: string): string[] {
	const out: string[] = [];
	const re = /"((?:[^"\\]|\\.)*)"|'([^']*)'|(\S+)/g;
	for (const m of line.matchAll(re)) out.push(m[1] ?? m[2] ?? m[3]);
	return out;
}

/** The commands of a script, one per line, with continued lines joined and `&&`, `;` and `&` splitting. */
function commandLines(text: string): { at: number; text: string }[] {
	const out: { at: number; text: string }[] = [];
	const lines = text.split(/\r?\n/);
	for (let i = 0; i < lines.length; i++) {
		const at = i + 1;
		let line = lines[i];
		// A backslash (shells) or backtick (PowerShell) at the end continues the line.
		while (/[\\`]\s*$/.test(line) && i + 1 < lines.length) line = line.replace(/[\\`]\s*$/, ' ') + lines[++i];
		line = line.replace(/^\s*(?:\$|>|PS [^>]*>)\s+/, '');
		if (/^\s*#/.test(line)) continue;
		for (const part of line.split(/\s(?:&&|\|\||;|&)(?=\s|$)|;\s/)) {
			if (part.trim()) out.push({ at, text: part.trim() });
		}
	}
	return out;
}

/** The favorites in `kubectl port-forward` commands. */
export function parseCommands(text: string): { entries: ForwardEntry[]; problems: ImportProblem[] } {
	const entries: ForwardEntry[] = [];
	const problems: ImportProblem[] = [];
	for (const { at, text: line } of commandLines(text)) {
		const all = words(line);
		const verb = all.indexOf('port-forward');
		if (verb === -1) continue;
		const problem = (reason: string) => problems.push({ at, text: line, reason });
		let namespace: string | null = null;
		let context: string | null = null;
		let address = '';
		const positional: string[] = [];
		// Options may come before and after `port-forward`; the program and its words before are skipped.
		const before = all.slice(0, verb);
		const program = before.findIndex((w) => /(^|[\\/])(kubectl|oc|k)(\.exe)?$/i.test(w));
		const args = [...before.slice(program + 1), ...all.slice(verb + 1)];
		for (let i = 0; i < args.length; i++) {
			const word = args[i];
			if (word === '--') {
				positional.push(...args.slice(i + 1));
				break;
			}
			if (!word.startsWith('-') || word === '-') {
				// Words before `port-forward` that are no option are not the target.
				if (i >= before.length - program - 1) positional.push(word);
				continue;
			}
			const eq = word.indexOf('=');
			let flag = eq === -1 ? word : word.slice(0, eq);
			let value: string | null = eq === -1 ? null : word.slice(eq + 1);
			// `-nshop` is `-n shop`.
			if (/^-n[^=-]/.test(word)) {
				flag = '-n';
				value = word.slice(2);
			}
			if (value === null && VALUE_FLAGS.has(flag)) value = args[++i] ?? '';
			if (flag === '-n' || flag === '--namespace') namespace = value || null;
			else if (flag === '--context') context = value || null;
			else if (flag === '--address') address = (value ?? '').split(',')[0].trim();
		}
		const [target, ...ports] = positional;
		if (!target) {
			problem(t('names no target'));
			continue;
		}
		const slash = target.indexOf('/');
		const kind = slash === -1 ? 'pods' : forwardKind(target.slice(0, slash));
		const name = slash === -1 ? target : target.slice(slash + 1);
		if (!kind || !name) {
			problem(t('cannot forward to {target}', { target }));
			continue;
		}
		if (!ports.length) {
			problem(t('names no port'));
			continue;
		}
		for (const spec of ports) {
			const m = /^(\d*):(.+)$/.exec(spec) ?? [spec, null, spec];
			const remote = portValue(m[2]);
			const local = m[1] === null ? (typeof remote === 'number' ? remote : null) : localValue(m[1]);
			if (remote === null || local === undefined) {
				problem(t('{port} is not a port', { port: spec }));
				continue;
			}
			// One port alone listens on the same port locally, as in kubectl, unless it needs admin rights.
			const own = m[1] === null && typeof local === 'number' && local < 1024 ? null : local;
			entries.push(entry(kind, name, remote, { namespace, local: own, address: address === '127.0.0.1' ? '' : address, context }));
		}
	}
	return { entries, problems };
}

/** The first line of an exported favorites file. */
const FILE_HEADER = '# Port-forward favorites for Kubri. Import them on the port-forwards page.';

/** A favorites file, or null when the text is none. */
function readFile(text: string): { entries: ForwardEntry[]; problems: ImportProblem[] } | null {
	let doc: unknown;
	try {
		doc = parse(text);
	} catch {
		return null;
	}
	const list = Array.isArray(doc) ? doc : doc && typeof doc === 'object' ? (doc as { forwards?: unknown }).forwards : undefined;
	if (!Array.isArray(list)) return null;
	const entries: ForwardEntry[] = [];
	const problems: ImportProblem[] = [];
	list.forEach((item: unknown, i) => {
		const at = i + 1;
		const shown = typeof item === 'object' && item ? JSON.stringify(item) : String(item);
		const problem = (reason: string) => problems.push({ at, text: shown, reason });
		if (!item || typeof item !== 'object') {
			problem(t('is not a favorite'));
			return;
		}
		const f = item as Record<string, unknown>;
		const target = typeof f.target === 'string' ? f.target.trim() : '';
		const slash = target.indexOf('/');
		const kind = slash === -1 ? null : forwardKind(target.slice(0, slash));
		const name = target.slice(slash + 1);
		if (!kind || !name) {
			problem(target ? t('cannot forward to {target}', { target }) : t('names no target'));
			return;
		}
		const remote = f.port === undefined ? null : portValue(String(f.port));
		if (remote === null) {
			problem(f.port === undefined ? t('names no port') : t('{port} is not a port', { port: String(f.port) }));
			return;
		}
		const local = localValue(f.localPort);
		if (local === undefined) {
			problem(t('{port} is not a port', { port: String(f.localPort) }));
			return;
		}
		const text = (key: string) => (typeof f[key] === 'string' ? (f[key] as string).trim() : '');
		entries.push(
			entry(kind, name, remote, {
				namespace: text('namespace') || null,
				local,
				address: text('address') === '127.0.0.1' ? '' : text('address'),
				label: text('name'),
				startOnConnect: typeof f.startOnConnect === 'boolean' ? f.startOnConnect : null
			})
		);
	});
	return { entries, problems };
}

/** Writes favorites as a file to share, which {@link parseImport} reads back. */
export function exportFavorites(favorites: FavoriteForward[]): string {
	const forwards = favorites.map((f) => {
		const label = (f as FavoriteForward & { name?: string }).name;
		return {
			...(label ? { name: label } : {}),
			...(f.target.namespace ? { namespace: f.target.namespace } : {}),
			target: `${singular(f.target.resource.plural)}/${f.target.name}`,
			port: f.remotePort,
			localPort: f.localPort,
			...(f.address ? { address: f.address } : {}),
			startOnConnect: f.startOnConnect
		};
	});
	return `${FILE_HEADER}\n${stringify({ forwards })}`;
}

/** `service` for `services`, as kubectl writes a target. */
function singular(plural: string): string {
	return plural.endsWith('s') && KIND_NAMES[plural.slice(0, -1)] === plural ? plural.slice(0, -1) : plural;
}

/**
 * The favorites for objects: one per TCP port they declare, on the local port a k9s
 * `k9scli.io/port-forwards` annotation names, else on one Kubri chooses.
 */
export function entriesOf(objects: { kind: ForwardKind; object: KubeObject }[]): ForwardEntry[] {
	return objects.flatMap(({ kind, object }) => {
		const wanted = new Map(annotatedForwards(object, PORT_FORWARDS_KEY).map((a) => [a.remote, a.local]));
		return forwardablePorts(object).map((p) =>
			entry(kind, object.metadata.name ?? '', p.port, {
				namespace: object.metadata.namespace ?? null,
				local: wanted.get(p.port) ?? null
			})
		);
	});
}

/**
 * The port of an object an entry means: its number, or the number of the port with its name.
 * A Service must have the port; a pod or workload may listen on one it does not declare.
 */
export function remotePortOf(e: Pick<ForwardEntry, 'kind' | 'remote'>, object: KubeObject): number | string {
	const ports = forwardablePorts(object);
	if (typeof e.remote === 'string') {
		const port = ports.find((p) => p.name === e.remote)?.port;
		return port ?? t('has no port named {port}', { port: e.remote });
	}
	if (e.kind === 'services' && !ports.some((p) => p.port === e.remote)) return t('has no port {port}', { port: e.remote });
	return e.remote;
}

/** Why a planned favorite needs a look before it is kept. */
export type PortConflict = 'twice' | 'favorite' | 'inUse';

export interface PlannedPort {
	local: number;
	/** Chosen by Kubri, not given. */
	chosen: boolean;
	conflict: PortConflict | null;
}

/** The lowest port {@link assignPorts} counts up from when the usual ones are taken. */
export const COUNT_FROM = 20000;

/**
 * Local ports for planned favorites. A given port stays and is marked if another favorite or
 * something else already uses it. Kubri chooses a free one otherwise: the remote port itself where
 * it needs no admin rights, else the remote port plus 10000 (80 becomes 10080), else the first free
 * port from 20000 up.
 *
 * `favorites` are the local ports of favorites already kept, `inUse` those something listens on.
 */
export function assignPorts(
	wanted: { remote: number; local: number | null }[],
	favorites: Iterable<number>,
	inUse: Iterable<number>
): PlannedPort[] {
	const kept = new Set(favorites);
	const busy = new Set(inUse);
	const used = new Set<number>();
	const out: (PlannedPort | null)[] = wanted.map(() => null);
	// Given ports first, so a chosen one never takes a port a later line asks for.
	wanted.forEach((w, i) => {
		if (w.local === null) return;
		const conflict: PortConflict | null = used.has(w.local) ? 'twice' : kept.has(w.local) ? 'favorite' : busy.has(w.local) ? 'inUse' : null;
		used.add(w.local);
		out[i] = { local: w.local, chosen: false, conflict };
	});
	const free = (port: number) => port >= 1024 && port <= 65535 && !used.has(port) && !kept.has(port) && !busy.has(port);
	let next = COUNT_FROM;
	wanted.forEach((w, i) => {
		if (w.local !== null) return;
		let local = [w.remote, w.remote + 10000].find(free);
		if (local === undefined) {
			while (next <= 65535 && !free(next)) next++;
			local = next;
		}
		used.add(local);
		out[i] = { local, chosen: true, conflict: null };
	});
	return out as PlannedPort[];
}

/** The target of an entry in the cluster's terms. */
export function refOfKind(resources: ResourceInfo[], kind: ForwardKind): ResourceRef | null {
	const r = resourceOf(resources, kind);
	return r ? { group: r.group, version: r.version, plural: r.plural } : null;
}

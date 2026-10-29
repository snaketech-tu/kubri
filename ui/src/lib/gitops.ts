import { backend, type Health, type Manager, type ObjectRef, type ResourceRef } from './api';

/** Flux kinds with `spec.suspend`, by plural. */
const SUSPENDABLE = new Set([
	'kustomizations',
	'helmreleases',
	'gitrepositories',
	'helmrepositories',
	'ocirepositories',
	'buckets',
	'helmcharts',
	'imagerepositories',
	'imageupdateautomations',
	'alerts',
	'providers',
	'receivers'
]);

export const isFlux = (resource: ResourceRef) => resource.group.endsWith('.toolkit.fluxcd.io');
export const isArgoApp = (resource: ResourceRef) => resource.group === 'argoproj.io' && resource.plural === 'applications';
export const canSuspendFlux = (resource: ResourceRef) => isFlux(resource) && SUSPENDABLE.has(resource.plural);

/** An object a Kustomization or Application applied, as its status lists it. */
export interface ManagedObject {
	group: string;
	version: string;
	kind: string;
	namespace: string | null;
	name: string;
	/** Argo CD: Synced or OutOfSync. */
	sync?: string;
	health?: string;
	message?: string;
}

export interface GitOpsStatus {
	state: string;
	health: Health | null;
	message: string | null;
	facts: { label: string; value: string; time?: boolean }[];
	managed: ManagedObject[];
}

type Json = Record<string, unknown>;
const get = (object: unknown, path: string): unknown =>
	path.split('.').reduce<unknown>((at, key) => (at && typeof at === 'object' ? (at as Json)[key] : undefined), object);
const text = (object: unknown, path: string) => {
	const value = get(object, path);
	return typeof value === 'string' && value ? value : null;
};

interface Condition {
	type: string;
	status: string;
	reason?: string;
	message?: string;
	lastTransitionTime?: string;
}

/** The state of a Flux object or an Argo CD Application, from its status; null for other kinds. */
export function gitopsStatus(resource: ResourceRef, object: unknown): GitOpsStatus | null {
	if (isFlux(resource)) return fluxStatus(object);
	if (isArgoApp(resource)) return argoStatus(object);
	return null;
}

function fluxStatus(object: unknown): GitOpsStatus {
	const conditions = (get(object, 'status.conditions') as Condition[] | undefined) ?? [];
	const ready = conditions.find((c) => c.type === 'Ready');
	const stalled = conditions.find((c) => c.type === 'Stalled' && c.status === 'True');
	const suspended = get(object, 'spec.suspend') === true;
	let state = 'Not reconciled yet';
	let health: Health | null = null;
	if (stalled) [state, health] = [`Stalled: ${stalled.reason ?? 'unknown'}`, 'failed'];
	else if (ready?.status === 'True') [state, health] = ['Ready', 'ok'];
	else if (ready?.status === 'False') [state, health] = [`Failed: ${ready.reason ?? 'unknown'}`, 'failed'];
	else if (ready) [state, health] = ['Reconciling', 'warning'];
	if (suspended) [state, health] = [`Suspended · ${state}`, 'warning'];

	const facts: GitOpsStatus['facts'] = [];
	const add = (label: string, value: string | null, time = false) => value && facts.push({ label, value, time });
	const applied = text(object, 'status.lastAppliedRevision') ?? text(object, 'status.artifact.revision');
	const attempted = text(object, 'status.lastAttemptedRevision');
	const history = (get(object, 'status.history') as Json[] | undefined)?.[0];
	add('Revision', applied ?? (history ? `${history.chartName ?? ''} ${history.chartVersion ?? ''}`.trim() : null));
	if (attempted && attempted !== applied) add('Attempted', attempted);
	const source = (get(object, 'spec.sourceRef') ?? get(object, 'spec.chart.spec.sourceRef') ?? get(object, 'spec.chartRef')) as Json | undefined;
	if (source) {
		const chart = text(object, 'spec.chart.spec.chart');
		const version = text(object, 'spec.chart.spec.version');
		const ref = `${source.kind} ${source.namespace ? `${source.namespace}/` : ''}${source.name}`;
		add('Source', chart ? `${chart}${version ? ` ${version}` : ''} from ${ref}` : ref);
	}
	add('URL', text(object, 'spec.url'));
	const branch = (get(object, 'spec.ref') ?? {}) as Json;
	const refText = ['branch', 'tag', 'semver', 'commit', 'name'].map((k) => (branch[k] ? `${k} ${branch[k]}` : null)).find(Boolean) ?? null;
	add('Ref', refText);
	add('Path', text(object, 'spec.path'));
	add('Interval', text(object, 'spec.interval'));
	add('Last reconcile', ready?.lastTransitionTime ?? null, true);

	const entries = (get(object, 'status.inventory.entries') as { id: string; v: string }[] | undefined) ?? [];
	const managed = entries.flatMap((e): ManagedObject[] => {
		const [namespace, name, group, kind] = e.id.split('_');
		return kind ? [{ group, version: e.v, kind, namespace: namespace || null, name }] : [];
	});
	return { state, health, message: ready?.status === 'True' ? null : (stalled?.message ?? ready?.message ?? null), facts, managed };
}

function argoStatus(object: unknown): GitOpsStatus {
	const sync = text(object, 'status.sync.status') ?? 'Unknown';
	const healthState = text(object, 'status.health.status') ?? 'Unknown';
	const phase = text(object, 'status.operationState.phase');
	const failed = phase === 'Failed' || phase === 'Error';
	let health: Health = 'ok';
	if (failed || healthState === 'Degraded' || healthState === 'Missing') health = 'failed';
	else if (sync !== 'Synced' || healthState !== 'Healthy') health = 'warning';
	const conditions = (get(object, 'status.conditions') as { type: string; message?: string }[] | undefined) ?? [];
	const message = failed
		? (text(object, 'status.operationState.message') ?? conditions[0]?.message ?? null)
		: (conditions.find((c) => /Error$/.test(c.type))?.message ?? null);

	const facts: GitOpsStatus['facts'] = [];
	const add = (label: string, value: string | null, time = false) => value && facts.push({ label, value, time });
	const source = (get(object, 'spec.source') ?? (get(object, 'spec.sources') as unknown[] | undefined)?.[0]) as Json | undefined;
	add('Revision', text(object, 'status.sync.revision'));
	if (source) {
		add('Target', (source.targetRevision as string) || 'HEAD');
		const where = (source.chart as string) ?? (source.path as string) ?? '';
		add('Source', `${source.repoURL ?? ''}${where ? ` · ${where}` : ''}`);
	}
	const destination = (get(object, 'spec.destination') ?? {}) as Json;
	add('Destination', [destination.name ?? destination.server, destination.namespace].filter(Boolean).join(' · ') || null);
	const automated = get(object, 'spec.syncPolicy.automated') as Json | null | undefined;
	const autoOn = !!automated && automated.enabled !== false;
	const options = autoOn ? ['selfHeal', 'prune'].filter((k) => automated![k]).map((k) => (k === 'selfHeal' ? 'self-heal' : k)) : [];
	add('Auto-sync', autoOn ? `On${options.length ? ` (${options.join(', ')})` : ''}` : 'Off');
	if (phase) add('Last sync', `${phase}`);
	add('Synced at', text(object, 'status.operationState.finishedAt'), true);
	add('Compared at', text(object, 'status.reconciledAt'), true);

	const resources = (get(object, 'status.resources') as Json[] | undefined) ?? [];
	const managed = resources.map((r) => ({
		group: (r.group as string) ?? '',
		version: (r.version as string) ?? '',
		kind: r.kind as string,
		namespace: (r.namespace as string) || null,
		name: r.name as string,
		sync: r.status as string | undefined,
		health: (r.health as Json | undefined)?.status as string | undefined,
		message: (r.health as Json | undefined)?.message as string | undefined
	}));
	return { state: `${sync} · ${healthState}${phase && phase !== 'Succeeded' ? ` · sync ${phase.toLowerCase()}` : ''}`, health, message, facts, managed };
}

/** How the panel words what a manager does with changes made by hand. */
export function managerNote(m: Manager): string {
	const who = `${m.tool} ${m.kind} ${m.owner.name}`;
	if (m.paused) {
		return m.tool === 'Flux'
			? `Managed by ${who}, which is suspended: changes made here stay until it is resumed and reconciles again.`
			: `Managed by ${who}, whose auto-sync is off: changes made here stay until the next sync.`;
	}
	if (m.reverts) {
		return m.tool === 'Flux'
			? `Managed by ${who}: Flux puts changes made here back at its next reconcile. Change the source in Git, or suspend it first.`
			: `Managed by ${who}, which self-heals: Argo CD puts changes made here back right away. Change the source in Git, or turn auto-sync off first.`;
	}
	return `Managed by ${who}: changes made here stay until its next ${m.tool === 'Flux' ? 'upgrade' : 'sync'} from Git overwrites them.`;
}

const managers = new Map<string, { at: number; found: Promise<Manager | null> }>();

/** Who manages `target`, asked again after half a minute. */
export function managerOf(target: ObjectRef): Promise<Manager | null> {
	const key = `${target.context ?? ''}/${target.resource.group}/${target.resource.plural}/${target.namespace ?? ''}/${target.name}`;
	const known = managers.get(key);
	if (known && Date.now() - known.at < 30_000) return known.found;
	const found = backend.managedBy(target).catch(() => null);
	managers.set(key, { at: Date.now(), found });
	return found;
}

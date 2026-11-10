import { access, need } from './access.svelte';
import { canDelete, canEdit, canRestart, canScale, copyLines, cordon, portForward, resourcesOf, runGitOps, startPlugin } from './actions';
import { isCertificates } from './certs';
import { backend, type AccessCheck, type ObjectRef, type Owner, type ResourceInfo, type TerminalMode } from './api';
import { hasCharts } from './charts';
import { creating } from './create.svelte';
import { dock } from './dock.svelte';
import { errorMessage } from './format';
import { canSuspendFlux, isArgoApp, isFlux } from './gitops';
import { openGrafanaLogs } from './integrations';
import { parseShortcut, pluginApplies, shortcutLabel } from './k9s';
import { kubectlCommand, type KubectlOp } from './kubectl';
import { t } from './i18n';
import { actionFor, type MenuAction } from './menu';
import {
	canPortForward,
	hasImages,
	hasLogs,
	hasUsers,
	isCRDs,
	isCronJobs,
	isNamespaces,
	isNodes,
	isPods,
	isReplicaSets,
	isSecrets,
	isServices
} from './pods';
import { app, findResource, type DetailsExtra, type DetailsTab, type Dialog } from './state.svelte';

/** An object to act on, with what its actions need to know about it. */
export interface Subject {
	resource: ResourceInfo;
	target: ObjectRef;
	owners: Pick<Owner, 'apiVersion' | 'kind' | 'name' | 'controller'>[];
	/** The node a pod runs on. */
	node: string | null;
	/** Whether a CronJob is suspended. */
	suspended: boolean;
	/** Whether a node is cordoned; unknown when not given. */
	cordoned?: boolean;
	/** Whether a Service has no selector, so there are no pods to show; unknown when not given. */
	selectorless?: boolean;
}

/** Kinds Kubri drills into on Enter, and those `jumps.yaml` names. */
export function canDrill(resource: ResourceInfo): boolean {
	const builtin: Record<string, string[]> = {
		'': ['services', 'replicationcontrollers', 'nodes'],
		apps: ['deployments', 'replicasets', 'statefulsets', 'daemonsets'],
		batch: ['jobs', 'cronjobs']
	};
	if (builtin[resource.group]?.includes(resource.plural)) return true;
	return (app.k9s?.jumps ?? []).some((j) => {
		const parts = j.source.split('/');
		const [group, plural] = parts.length === 3 ? [parts[0], parts[2]] : ['', parts[1]];
		return group === resource.group && plural === resource.plural;
	});
}

/** What Enter shows for this kind, or null where Enter only describes. */
export function drillLabel(resource: ResourceInfo): string | null {
	if (isNamespaces(resource)) return t('Show pods');
	if (isCRDs(resource)) return t('Show objects');
	if (isPods(resource) || !canDrill(resource)) return null;
	if (isCronJobs(resource)) return t('Show jobs');
	const pods = ['services', 'replicationcontrollers', 'nodes', 'deployments', 'replicasets', 'statefulsets', 'daemonsets', 'jobs'];
	return pods.includes(resource.plural) ? t('Show pods') : t('Open');
}

/**
 * Enter, as in k9s: from a namespace to its pods, from a CRD to its objects, from a workload,
 * CronJob, Service or node to what belongs to it (or where `jumps.yaml` says). False when there
 * is nothing to drill into, for the caller to describe the object instead.
 */
export async function drill(resource: ResourceInfo, target: ObjectRef): Promise<boolean> {
	if (isNamespaces(resource)) {
		const pods = findResource(app.resources, { group: '', version: 'v1', plural: 'pods' });
		if (pods) {
			app.open(pods, target.name, '');
			return true;
		}
	}
	if (isCRDs(resource)) {
		const [plural, ...group] = target.name.split('.');
		const defined = app.resources.find((r) => r.plural === plural && r.group === group.join('.'));
		if (defined) {
			app.open(defined, undefined, '');
			return true;
		}
	}
	if (!isPods(resource) && canDrill(resource)) {
		try {
			const found = await backend.drill(target);
			if (found) {
				app.drill(found);
				return true;
			}
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
			return true;
		}
	}
	return false;
}

/** The owner Shift+J jumps to: the controller, or else the first. */
export function ownerOf(subject: Subject) {
	return subject.owners.find((o) => o.controller) ?? subject.owners[0];
}

/** Shows the controller of an object, like k9s Shift+J. */
export function jumpToOwner(subject: Subject) {
	const owner = ownerOf(subject);
	if (!owner) {
		app.toast(t('{name} has no owner', { name: subject.target.name }), 'bad');
		return;
	}
	const group = owner.apiVersion.includes('/') ? owner.apiVersion.split('/')[0] : '';
	const kinds = app.resources.filter((r) => r.group === group && r.kind === owner.kind);
	const version = owner.apiVersion.split('/').pop();
	const kind = kinds.find((r) => r.version === version) ?? kinds[0];
	if (!kind) {
		app.toast(t('The cluster does not serve {kind}', { kind: owner.kind }), 'bad');
		return;
	}
	app.openAt(kind, subject.target.namespace, owner.name);
}

/** The ReplicaSets of a Deployment, like k9s `z`. */
export async function showReplicaSets(target: ObjectRef) {
	const replicasets = findResource(app.resources, { group: 'apps', version: 'v1', plural: 'replicasets' });
	if (!replicasets) return;
	try {
		const uid = (await backend.getObject(target)).metadata.uid;
		if (!uid) return;
		app.open(replicasets, target.namespace ?? undefined, '', {
			title: t('ReplicaSets of Deployment {name}', { name: target.name }),
			scope: {},
			ownerUid: uid
		});
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

export interface ActionOptions {
	/** Shows the object in the details panel. */
	show: (tab: DetailsTab, extra?: DetailsExtra) => void;
	/** Enter; left out where the object is already open. */
	enter?: () => void;
	/** The objects copying and deleting work on, when not just this one. */
	chosen?: ObjectRef[];
	/** The namespace the list shows, to leave out switching to it. */
	namespace?: string;
	/** Offers the k9s plugins of the view, after this puts the object under the cursor. */
	plugins?: () => void;
	/** More actions for the Delete group, like deleting finished pods. */
	extraDanger?: MenuAction[];
}

/**
 * Everything there is to do on an object, for the action bar, the row menu and the keys alike.
 * Where one key has several meanings, the first action with it wins, as in k9s.
 */
export function objectActions(subject: Subject, options: ActionOptions, writable = !app.readOnly): MenuAction[] {
	const { resource, target } = subject;
	const out: MenuAction[] = [];
	/** Adds an action; with `needs`, one RBAC refuses is offered disabled and explains why. */
	const add = (
		group: string,
		label: string,
		keys: string[],
		run: () => void,
		{ needs, ...more }: Partial<MenuAction> & { needs?: AccessCheck } = {}
	) => {
		const disabled = needs ? access.denial(needs) : null;
		const explain = () => (app.dialog = { kind: 'forbidden', action: label.replace(/…$/, ''), check: needs!, target });
		out.push({ group, label, keys, run: disabled ? explain : run, ...more, ...(disabled ? { disabled } : {}) });
	};
	const show = (tab: DetailsTab, extra: DetailsExtra = {}) => () => options.show(tab, extra);
	const dialog = (make: () => Dialog) => () => (app.dialog = make());
	const ns = target.namespace;
	/** RBAC's view of `verb` on this object, or on its subresource. */
	const on = (verb: string, subresource = '') => need(verb, resource, ns, { subresource, name: target.name });
	const pods = { group: '', plural: 'pods' };
	const k = (op: KubectlOp) => kubectlCommand(app.cluster?.context, target, op);

	const drillTo = drillLabel(resource);
	if (drillTo && options.enter && !(subject.selectorless && isServices(resource))) add('View', drillTo, [], options.enter, { hint: 'enter', primary: 1 });
	add('View', t('Describe'), ['d'], show('describe'), { primary: 5, id: 'describe', needs: on('get'), kubectl: k({ op: 'describe' }) });
	add('View', t('YAML'), ['y'], show('yaml'), { primary: 6, id: 'yaml', needs: on('get'), kubectl: k({ op: 'yaml' }) });
	const logs = isPods(resource) ? on('get', 'log') : need('get', pods, ns, { subresource: 'log' });
	if (hasCharts(resource) && (app.integrations.has('prometheus') || app.integrations.has('grafana'))) add('View', t('Metrics'), ['m'], show('metrics'), { primary: 9, id: 'metrics' });
	// Any object: its logs in Explore, the dashboards that fit it, and Grafana's search by name.
	const grafana = app.contextSettings.integrations?.grafana;
	if ((grafana && grafana !== 'off') || app.integrations.has('grafana')) {
		const here = { ...target, context: app.cluster?.context };
		if (ns || (!resource.group && resource.plural === 'namespaces')) add('View', t('Logs in Grafana'), [], () => openGrafanaLogs(here), { id: 'grafana-logs' });
		add('View', t('Grafana…'), [], dialog(() => ({ kind: 'grafana', target: here })), { id: 'grafana' });
	}
	if (hasLogs(resource)) add('View', t('Logs'), ['l'], show('logs'), { primary: 2, id: 'logs', needs: logs, kubectl: k({ op: 'logs', workload: !isPods(resource) }) });
	const keep = (kind: 'logs' | 'shell', extra: { previous?: boolean; mode?: TerminalMode } = {}) => () =>
		dock.add({ kind, resource, target: { ...target, context: app.cluster?.context }, ...extra });
	if (hasLogs(resource)) add('View', t('Logs in workspace'), [], keep('logs'), { needs: logs });
	if (isPods(resource)) add('View', t('Previous logs'), ['p'], show('logs', { previous: true }), { id: 'previous', needs: logs, kubectl: k({ op: 'logs', previous: true }) });
	if (isSecrets(resource)) add('View', t('Decode'), ['x'], show('describe', { decoded: true }), { needs: on('get'), kubectl: k({ op: 'decode' }) });
	if (hasUsers(resource)) add('View', t('Used by'), ['u'], show('describe'), { id: 'usedBy' });
	add('View', t('What changed'), [], () => app.showChanges({ namespace: target.namespace ?? undefined, kind: resource.kind, name: target.name }), { id: 'changes' });
	{
		const pin = app.pins.of(resource, target);
		add('View', app.pins.has(pin) ? t('Unpin from sidebar') : t('Pin to sidebar'), [], () => app.pins.toggle(pin), { id: 'pin' });
	}
	add('View', t('Compare…'), [], dialog(() => ({ kind: 'compare', resource, target })), { id: 'compare', needs: on('get') });
	// In the action bar once a model is set up; until then the menus lead to the settings.
	const explaining = app.prefs.settings.explain.provider !== 'none' ? { primary: isPods(resource) ? 4 : 9 } : {};
	add('View', t('Explain…'), ['E'], () => (app.explaining = { kind: 'object', target }), { id: 'explain', needs: on('get'), ...explaining });

	if (isPods(resource) && writable) {
		add('Connect', t('Shell'), ['s'], show('shell', { mode: 'shell' }), { primary: 3, id: 'shell', needs: on('create', 'exec'), kubectl: k({ op: 'shell' }) });
		add('Connect', t('Shell in workspace'), [], keep('shell', { mode: 'shell' }), { needs: on('create', 'exec') });
		add('Connect', t('Attach'), ['a'], show('shell', { mode: 'attach' }), { id: 'attach', needs: on('create', 'attach'), kubectl: k({ op: 'attach' }) });
		add('Connect', t('Debug…'), ['D'], dialog(() => ({ kind: 'debug', on: 'pod', target })), { id: 'debug', needs: on('patch', 'ephemeralcontainers'), kubectl: k({ op: 'debug' }) });
	}
	if (isNodes(resource) && writable) {
		add('Connect', t('Node shell…'), ['D'], dialog(() => ({ kind: 'debug', on: 'node', target })), { id: 'nodeShell', needs: need('create', pods, 'default'), kubectl: k({ op: 'nodeShell' }) });
	}
	if (canPortForward(resource)) {
		const forward = isPods(resource) ? on('create', 'portforward') : need('create', pods, ns, { subresource: 'portforward' });
		add('Connect', t('Port-forward…'), ['F'], () => void portForward(resource, target), { primary: 8, needs: forward, kubectl: k({ op: 'portForward' }) });
	}

	if (writable) {
		if (isNodes(resource)) {
			if (subject.cordoned !== true) add('Change', t('Cordon'), ['c'], () => void cordon(target.name, true), { primary: 4, needs: on('patch'), kubectl: k({ op: 'cordon' }) });
			if (subject.cordoned !== false) add('Change', t('Uncordon'), ['u'], () => void cordon(target.name, false), { primary: 4, needs: on('patch'), kubectl: k({ op: 'uncordon' }) });
			add('Change', t('Drain…'), ['r'], dialog(() => ({ kind: 'drain', resource, target })), { primary: 4, needs: on('patch'), kubectl: k({ op: 'drain', options: { ignoreDaemonsets: true } }) });
		}
		if (isCronJobs(resource)) {
			add('Change', t('Trigger now…'), ['t'], dialog(() => ({ kind: 'trigger', resource, target })), { primary: 4, needs: need('create', { group: 'batch', plural: 'jobs' }, ns), kubectl: k({ op: 'trigger' }) });
			const suspend = !subject.suspended;
			add('Change', suspend ? t('Suspend…') : t('Resume…'), ['s'], dialog(() => ({ kind: 'suspend', resource, target, suspend })), { needs: on('patch'), kubectl: k({ op: 'suspend', suspend }) });
		}
		if (isFlux(resource)) {
			add('Change', t('Reconcile'), ['r'], () => void runGitOps(target, 'reconcile'), { primary: 4, needs: on('patch') });
			if (canSuspendFlux(resource)) {
				const action = subject.suspended ? 'resume' : 'suspend';
				add('Change', subject.suspended ? t('Resume…') : t('Suspend…'), ['s'], dialog(() => ({ kind: 'gitops', resource, target, action })), { needs: on('patch') });
			}
		}
		if (isArgoApp(resource)) {
			add('Change', t('Sync…'), ['s'], dialog(() => ({ kind: 'gitops', resource, target, action: 'sync' })), { primary: 4, needs: on('patch') });
			add('Change', t('Refresh'), ['r'], () => void runGitOps(target, 'refresh'), { needs: on('patch') });
			add('Change', t('Auto-sync…'), ['t'], dialog(() => ({ kind: 'gitops', resource, target, action: 'autoSync' })), { needs: on('patch') });
		}
		if (isCertificates(resource)) {
			const status = need('patch', resource, ns, { subresource: 'status', name: target.name });
			add('Change', t('Renew now…'), ['r'], dialog(() => ({ kind: 'renew', resource, target })), { primary: 4, needs: status });
		}
		if (canEdit(resource)) add('Change', t('Edit'), ['e'], show('yaml', { editing: true }), { primary: 7, id: 'edit', needs: on('update'), kubectl: k({ op: 'edit' }) });
		if (canScale(resource) && !isPods(resource)) {
			add('Change', t('Scale…'), ['s'], dialog(() => ({ kind: 'scale', resource, target })), { primary: 4, needs: on('patch', 'scale'), kubectl: k({ op: 'scale' }) });
		}
		if (canRestart(resource)) add('Change', t('Restart…'), ['r'], dialog(() => ({ kind: 'restart', resource, target })), { primary: 4, needs: on('patch'), kubectl: k({ op: 'restart' }) });
		if (hasImages(resource)) add('Change', t('Set image…'), ['i'], dialog(() => ({ kind: 'image', resource, target })), { needs: on('patch'), kubectl: k({ op: 'image' }) });
		// The pod template's requests and limits; a pod's are changed on the workload that owns it.
		const sized = resource.group === 'apps' && ['deployments', 'statefulsets', 'daemonsets'].includes(resource.plural);
		if (sized || (isPods(resource) && ownerOf(subject))) {
			add('Change', t('Requests & limits…'), ['Q'], () => void resourcesOf(target), {
				id: 'resources',
				...(sized ? { needs: on('patch'), kubectl: k({ op: 'resources', containers: [] }) } : {})
			});
		}
		if (isReplicaSets(resource)) {
			add('Change', t('Roll back to this…'), ['ctrl+l'], dialog(() => ({ kind: 'rollback', resource, target })), { needs: need('update', { group: 'apps', plural: 'deployments' }, ns), kubectl: k({ op: 'rollback', deployment: ownerOf(subject)?.name }) });
		}
		if (resource.verbs.includes('patch')) {
			add('Change', t('Labels & annotations…'), ['L'], dialog(() => ({ kind: 'labels', resource, target })), { needs: on('patch'), kubectl: k({ op: 'labels' }) });
		}
		if (resource.verbs.includes('create') && resource.kind !== 'Event') add('Change', t('Clone to…'), [], () => void cloneTo(target));
	}

	const nodes = findResource(app.resources, { group: '', version: 'v1', plural: 'nodes' });
	const node = subject.node;
	if (node && node !== '<none>' && nodes) add('Go to', t('Node {name}', { name: node }), ['o'], () => app.openAt(nodes, null, node));
	const owner = ownerOf(subject);
	if (owner) add('Go to', `${owner.kind} ${owner.name}`, ['J'], () => jumpToOwner(subject));
	if (resource.group === 'apps' && resource.plural === 'deployments') {
		add('Go to', 'ReplicaSets', ['z'], () => void showReplicaSets(target));
	}
	if (isNamespaces(resource) && app.cluster && !app.cluster.snapshot && !app.production) {
		// A setting in Kubri, not a change to the cluster: offered also when changes are off.
		const rule = app.productionRuleOf(target.name);
		if (rule && rule.by !== 'name') {
			add('Change', t('Production by the rule {rule}', { rule: rule.rule }), [], () => app.showConfig('contexts'), { id: 'production' });
		} else {
			add('Change', rule ? t('Unmark as production') : t('Mark as production'), [], () => void app.setProductionNamespace(target.name, !rule), {
				id: 'production',
				primary: 4
			});
		}
	}
	if (isNamespaces(resource)) {
		add('Go to', t('Switch to {name}', { name: target.name }), ['u'], () => {
			app.namespace = target.name;
			app.toast(t('Namespace {name}', { name: target.name }));
		});
	} else if (target.namespace && target.namespace !== options.namespace) {
		const ns = target.namespace;
		add('Go to', t('Switch to namespace {name}', { name: ns }), ['W'], () => (app.namespace = ns));
	}

	const chosen = options.chosen?.length ? options.chosen : [target];
	const several = chosen.length > 1;
	add('Copy', several ? t('Copy {n} names', { n: chosen.length }) : t('Copy name'), ['c'], () =>
		copyLines(
			chosen.map((c) => c.name),
			(n) => t('Copied {n} names', { n })
		)
	);
	if (resource.namespaced) {
		add('Copy', several ? t('Copy namespaces') : t('Copy namespace'), ['n'], () =>
			copyLines(
				[...new Set(chosen.map((c) => c.namespace ?? ''))],
				(n) => t('Copied {n} namespaces', { n })
			)
		);
	}

	add('Copy', t('Copy as kubectl…'), [], () => {
		const commands = out.filter((a) => a.kubectl).map((a) => ({ label: a.label.replace(/…$/, ''), command: a.kubectl! }));
		app.dialog = { kind: 'kubectl', title: target.name, commands };
	});

	const focus = options.plugins;
	if (focus) {
		for (const plugin of app.k9s?.plugins ?? []) {
			if (!pluginApplies(plugin, app.pluginView) || (plugin.dangerous && !writable)) continue;
			const shortcut = parseShortcut(plugin.shortCut);
			const run = () => {
				focus();
				startPlugin(plugin);
			};
			add('Plugins', plugin.description || plugin.name, [], run, { hint: shortcut ? shortcutLabel(shortcut) : undefined });
		}
	}

	if (writable) out.push(...(options.extraDanger ?? []));
	if (writable && canDelete(resource)) {
		const label = several ? t('Delete {n}…', { n: chosen.length }) : t('Delete…');
		const run = dialog(() => ({ kind: 'delete', resource, targets: chosen }));
		// For several objects, the cursor's stands for all: they share a kind and mostly a namespace.
		add('Delete', label, ['ctrl+d', 'Delete'], run, { danger: true, primary: 0, needs: on('delete'), kubectl: k({ op: 'delete', targets: chosen }) });
	}
	return out;
}

/**
 * The action a key would run if Kubri were not read-only, for saying why nothing happens.
 * Undefined when changes are allowed, or the key means nothing here either way.
 */
export function refusedAction(subject: Subject, options: ActionOptions, key: string): MenuAction | undefined {
	if (!app.readOnly) return undefined;
	return actionFor(objectActions(subject, options, true), key);
}

/** The subject of an object as the API returns it. */
export function subjectOf(resource: ResourceInfo, target: ObjectRef, object: { metadata: { ownerReferences?: { apiVersion?: string; kind: string; name: string; controller?: boolean }[] }; spec?: unknown } | null): Subject {
	const spec = (object?.spec ?? {}) as { nodeName?: string; suspend?: boolean; unschedulable?: boolean; selector?: Record<string, string> };
	return {
		resource,
		target,
		owners: (object?.metadata.ownerReferences ?? []).map((o) => ({
			apiVersion: o.apiVersion ?? '',
			kind: o.kind,
			name: o.name,
			controller: o.controller ?? false
		})),
		node: isPods(resource) ? (spec.nodeName ?? null) : null,
		suspended: spec.suspend ?? false,
		...(object && isNodes(resource) ? { cordoned: spec.unschedulable ?? false } : {}),
		...(object && isServices(resource) ? { selectorless: !spec.selector || !Object.keys(spec.selector).length } : {})
	};
}

/**
 * Opens the menu of an object outside the resource list, as in XRay or the sanitizer, whose keys
 * mean other things: so the menu shows no keys. `before` puts the view's cursor on the object, and
 * `first` are the view's own actions for it.
 */
export function openObjectMenu(
	resource: ResourceInfo,
	target: ObjectRef,
	x: number,
	y: number,
	{ before, first = [] }: { before?: () => void; first?: MenuAction[] } = {}
) {
	const show = (tab: DetailsTab, extra: DetailsExtra = {}) => {
		before?.();
		app.showDetails(resource, target, tab, extra);
	};
	const subject: Subject = { resource, target, owners: [], node: null, suspended: false };
	const enter = drillLabel(resource) ? () => void drill(resource, target) : undefined;
	const actions = objectActions(subject, { show, enter });
	// With the other places to go, after viewing and changing it.
	const at = actions.findIndex((a) => !['View', 'Connect', 'Change'].includes(a.group));
	const list = { group: 'Go to', label: t('Show in the {kind} list', { kind: resource.kind }), keys: [], run: () => app.openAt(resource, target.namespace, target.name) };
	actions.splice(at === -1 ? actions.length : at, 0, list);
	before?.();
	app.menu = { actions: [...first, ...actions], x, y, hints: false };
}

/** Opens the Create page with a copy of the object, for another namespace or context. */
async function cloneTo(target: ObjectRef) {
	try {
		creating.clone(await backend.getObject(target), null);
		app.showCreate();
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

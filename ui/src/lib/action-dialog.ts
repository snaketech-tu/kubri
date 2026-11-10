/** What the action dialog says and asks, worked out without rendering it. */
import type { Change, ContainerImage, ContainerResources, DeleteOptions, DiffSettings, DrainOptions, DrainReport, Reference } from './api';
import { validReplicas } from './forms';
import { t } from './i18n';
import { helmCommand, kubectlCommand } from './kubectl';
import { summarizeUsers } from './references';
import type { Dialog } from './state.svelte';

/** The dialogs ActionDialog shows: those that review a change and then run it. */
export type ActionDialogSpec = Exclude<Dialog, { kind: 'forbidden' | 'kubectl' | 'debug' | 'compare' | 'snapshot' | 'grafana' }>;

/** Whether Argo CD's auto-sync is on now, for the action that turns it off or on. */
export type AutoSync = { on: boolean; options: string[] };

/** `namespace/name`, or the name of an object without a namespace. */
export function targetName(target: { namespace: string | null; name: string }) {
	return target.namespace ? `${target.namespace}/${target.name}` : target.name;
}

export function dialogTitle(dialog: ActionDialogSpec): string {
	switch (dialog.kind) {
		case 'delete':
			return t('Delete {kind}', { kind: dialog.resource.kind });
		case 'scale':
			return t('Scale {kind}', { kind: dialog.resource.kind });
		case 'restart':
			return t('Restart {kind}', { kind: dialog.resource.kind });
		case 'scaleMany':
			return t('Scale {n} {kind} objects', { n: dialog.targets.length, kind: dialog.resource.kind });
		case 'restartMany':
			return t('Restart {n} {kind} objects', { n: dialog.targets.length, kind: dialog.resource.kind });
		case 'labels':
			return t('Labels and annotations');
		case 'portforward':
			return t('Port-forward');
		case 'drain':
			return t('Drain node');
		case 'benchmark':
			return t('HTTP benchmark');
		case 'helmRollback':
			return t('Roll back release');
		case 'helmUninstall':
			return t('Uninstall release');
		case 'plugin':
			return dialog.plugin.description || dialog.plugin.name;
		case 'trigger':
			return t('Run CronJob now');
		case 'suspend':
			return dialog.suspend ? t('Suspend CronJob') : t('Resume CronJob');
		case 'renew':
			return t('Renew certificate');
		case 'gitops': {
			const kind = dialog.resource.kind;
			return {
				autoSync: () => t('Auto-sync'),
				autoSyncOff: () => t('Auto-sync'),
				autoSyncOn: () => t('Auto-sync'),
				reconcile: () => t('Reconcile {kind}', { kind }),
				suspend: () => t('Suspend {kind}', { kind }),
				resume: () => t('Resume {kind}', { kind }),
				sync: () => t('Sync {kind}', { kind }),
				refresh: () => t('Refresh {kind}', { kind })
			}[dialog.action]();
		}
		case 'image':
			return t('Set image');
		case 'rollback':
			return t('Roll back {kind}', { kind: dialog.owner?.kind ?? 'Deployment' });
		case 'memory':
			return t('Raise memory limit');
		case 'resources':
			return t('Requests & limits');
	}
}

/** The label of the button that runs the action. */
export function confirmLabel(dialog: ActionDialogSpec, autoSync: AutoSync | null): string {
	if (dialog.kind === 'suspend') return dialog.suspend ? t('Suspend') : t('Resume');
	if (dialog.kind === 'gitops') {
		return {
			autoSync: () => (autoSync?.on ? t('Turn off') : t('Turn on')),
			autoSyncOff: () => t('Turn off'),
			autoSyncOn: () => t('Turn on'),
			reconcile: () => t('Reconcile'),
			suspend: () => t('Suspend'),
			resume: () => t('Resume'),
			sync: () => t('Sync'),
			refresh: () => t('Refresh')
		}[dialog.action]();
	}
	return {
		delete: t('Delete'),
		scale: t('Scale'),
		restart: t('Restart'),
		scaleMany: t('Scale all'),
		restartMany: t('Restart all'),
		labels: t('Save'),
		portforward: t('Start'),
		drain: t('Drain'),
		benchmark: t('Start'),
		helmRollback: t('Roll back'),
		helmUninstall: t('Uninstall'),
		plugin: t('Run'),
		trigger: t('Run'),
		renew: t('Renew'),
		image: t('Set'),
		rollback: t('Roll back'),
		memory: t('Set limit'),
		resources: t('Set')
	}[dialog.kind];
}

/** Whether the action destroys something, so Enter runs it only from its own button. */
export function isDangerous(dialog: ActionDialogSpec, report: DrainReport | null): boolean {
	return (
		dialog.kind === 'delete' ||
		dialog.kind === 'helmUninstall' ||
		(dialog.kind === 'drain' && !report) ||
		(dialog.kind === 'plugin' && dialog.plugin.dangerous)
	);
}

/** The namespaces the action reaches, to see whether one is marked as production. */
export function reachedNamespaces(dialog: ActionDialogSpec): (string | null | undefined)[] {
	if (dialog.kind === 'delete') {
		// Deleting a namespace takes everything in it.
		const namespaces = !dialog.resource.group && dialog.resource.plural === 'namespaces';
		return dialog.targets.map((target) => (namespaces ? target.name : target.namespace));
	}
	if (dialog.kind === 'helmUninstall' || dialog.kind === 'helmRollback') return [dialog.release.namespace];
	if (dialog.kind === 'plugin') return [dialog.variables.NAMESPACE];
	if ('target' in dialog) return [dialog.target.namespace];
	return [];
}

/**
 * What to type before a destructive action: on a production context or namespace the object's
 * name (the context's for several objects or a plugin), elsewhere "confirm" for bulk deletes.
 * Null when the button is enough.
 */
export function confirmWord(dialog: ActionDialogSpec, danger: boolean, production: boolean, context: string | undefined): string | null {
	if (!danger) return null;
	if (production) {
		if (dialog.kind === 'delete') return dialog.targets.length === 1 ? dialog.targets[0].name : (context ?? 'confirm');
		if (dialog.kind === 'helmUninstall') return dialog.release.name;
		if (dialog.kind === 'drain') return dialog.target.name;
		return context ?? 'confirm';
	}
	return dialog.kind === 'delete' && dialog.targets.length > 1 ? 'confirm' : null;
}

/** Labels or annotations as `key=value` lines, without kubectl's last applied configuration. */
export function toLines(map: Record<string, string>) {
	return Object.entries(map)
		.filter(([k]) => k !== 'kubectl.kubernetes.io/last-applied-configuration')
		.map(([k, v]) => `${k}=${v}`)
		.join('\n');
}

/** Reads `key=value` lines back; throws on a line without a key. */
export function fromLines(text: string): Record<string, string> {
	const out: Record<string, string> = {};
	for (const line of text.split('\n')) {
		const trimmed = line.trim();
		if (!trimmed) continue;
		const i = trimmed.indexOf('=');
		if (i <= 0) throw new Error(t('Expected key=value, got “{line}”', { line: trimmed }));
		out[trimmed.slice(0, i).trim()] = trimmed.slice(i + 1).trim();
	}
	return out;
}

/** A merge patch that sets changed keys and removes deleted ones. */
export function mapPatch(before: Record<string, string>, after: Record<string, string>) {
	const patch: Record<string, string | null> = {};
	for (const key of Object.keys(before)) {
		if (!(key in after) && key !== 'kubectl.kubernetes.io/last-applied-configuration') patch[key] = null;
	}
	for (const [key, value] of Object.entries(after)) {
		if (before[key] !== value) patch[key] = value;
	}
	return patch;
}

/** The labels and annotations patch as typed, or null while a line does not parse. */
export function metadataPatch(original: { labels: Record<string, string>; annotations: Record<string, string> }, labels: string, annotations: string) {
	try {
		return { labels: mapPatch(original.labels, fromLines(labels)), annotations: mapPatch(original.annotations, fromLines(annotations)) };
	} catch {
		return null;
	}
}

/** The containers whose image was changed, with the image typed in. */
export function changedImages(images: ContainerImage[] | null, typed: Record<string, string>): ContainerImage[] {
	return (images ?? []).map((c) => ({ ...c, image: typed[c.name]?.trim() ?? c.image })).filter((c, i) => c.image !== images?.[i].image);
}

/** What uses an object about to be deleted, and that it breaks without it. */
export function usedByText(user: { target: string; refs: Reference[] }, single: boolean) {
	const users = summarizeUsers(user.refs);
	const one = user.refs.length === 1;
	if (single) {
		return one
			? t('It is used by {users}, which breaks or stops starting without it:', { users })
			: t('It is used by {users}, which break or stop starting without it:', { users });
	}
	return one
		? t('{name} is used by {users}, which breaks or stops starting without it:', { name: user.target, users })
		: t('{name} is used by {users}, which break or stop starting without it:', { name: user.target, users });
}

/** What the dialog's fields say now. */
export interface ActionFields {
	replicas: number | null;
	deleteOptions: DeleteOptions;
	/** The labels and annotations patch, or null while a line does not parse. */
	metadata: { labels: Record<string, string | null>; annotations: Record<string, string | null> } | null;
	forward: { local: number | null; remote: number | null; address: string };
	drain: DrainOptions;
	/** The containers whose image changes. */
	images: ContainerImage[];
	memory: string;
	resources: ContainerResources[];
	/** Whether the requests and limits as typed have a problem. */
	resourceProblems: boolean;
}

/** The change to dry-run before the action runs, when the settings ask for a diff. */
export function previewChange(dialog: ActionDialogSpec, wanted: DiffSettings, f: ActionFields): Change | null {
	switch (dialog.kind) {
		case 'scale':
			return wanted.scale && validReplicas(f.replicas) ? { type: 'scale', replicas: f.replicas } : null;
		case 'restart':
			return wanted.restart ? { type: 'restart' } : null;
		case 'image':
			return wanted.image && f.images.length && f.images.every((c) => c.image) ? { type: 'images', images: f.images } : null;
		case 'labels':
			return wanted.labels && f.metadata ? { type: 'patch', patch: { metadata: f.metadata } } : null;
		case 'resources':
			// Always: the diff shows the pods will be replaced.
			return f.resources.length && !f.resourceProblems ? { type: 'resources', containers: f.resources } : null;
		case 'memory':
			// Always: the diff shows the pods will be replaced.
			return f.memory ? { type: 'memory', container: dialog.fix.container, memory: f.memory } : null;
		default:
			return null;
	}
}

/** The same as a kubectl or helm command, as the fields now say; null for actions without one. */
export function actionCommand(dialog: ActionDialogSpec, context: string | undefined, f: ActionFields): string | null {
	switch (dialog.kind) {
		case 'delete':
			return kubectlCommand(context, dialog.targets[0], { op: 'delete', targets: dialog.targets, options: f.deleteOptions });
		case 'scale':
			return kubectlCommand(context, dialog.target, { op: 'scale', replicas: f.replicas });
		case 'restart':
			return kubectlCommand(context, dialog.target, { op: 'restart' });
		case 'labels':
			return kubectlCommand(context, dialog.target, { op: 'labels', ...(f.metadata ?? {}) });
		case 'portforward':
			return kubectlCommand(context, dialog.target, { op: 'portForward', ...f.forward });
		case 'drain':
			return kubectlCommand(context, dialog.target, { op: 'drain', options: f.drain });
		case 'trigger':
			return kubectlCommand(context, dialog.target, { op: 'trigger' });
		case 'suspend':
			return kubectlCommand(context, dialog.target, { op: 'suspend', suspend: dialog.suspend });
		case 'image':
			return kubectlCommand(context, dialog.target, { op: 'image', images: f.images });
		case 'rollback':
			return kubectlCommand(context, dialog.target, {
				op: 'rollback',
				owner: dialog.owner ? `${dialog.owner.plural}.apps/${dialog.owner.name}` : null,
				revision: dialog.revision ? String(dialog.revision) : null
			});
		case 'memory':
			return kubectlCommand(context, dialog.target, { op: 'memory', container: dialog.fix.container, memory: f.memory });
		case 'resources':
			return kubectlCommand(context, dialog.target, { op: 'resources', containers: f.resources });
		case 'helmRollback':
			return helmCommand(context, dialog.release, { op: 'rollback', revision: dialog.revision });
		case 'helmUninstall':
			return helmCommand(context, dialog.release, { op: 'uninstall' });
		default:
			return null;
	}
}

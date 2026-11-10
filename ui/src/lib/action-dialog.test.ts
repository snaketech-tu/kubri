import { describe, expect, it } from 'vitest';
import {
	actionCommand,
	changedImages,
	confirmLabel,
	confirmWord,
	dialogTitle,
	fromLines,
	isDangerous,
	mapPatch,
	metadataPatch,
	previewChange,
	reachedNamespaces,
	toLines,
	type ActionDialogSpec,
	type ActionFields
} from './action-dialog';
import type { DiffSettings, DrainReport, ObjectRef, ResourceInfo } from './api';

const deployments = { group: 'apps', version: 'v1', plural: 'deployments', kind: 'Deployment', namespaced: true } as ResourceInfo;
const namespaces = { group: '', version: 'v1', plural: 'namespaces', kind: 'Namespace', namespaced: false } as ResourceInfo;
const web: ObjectRef = { resource: { group: 'apps', version: 'v1', plural: 'deployments' }, namespace: 'shop', name: 'web' };
const api: ObjectRef = { ...web, name: 'api' };
const shop: ObjectRef = { resource: { group: '', version: 'v1', plural: 'namespaces' }, namespace: null, name: 'shop' };

const fields: ActionFields = {
	replicas: 3,
	deleteOptions: { force: false, gracePeriodSeconds: null, propagation: 'background' },
	metadata: null,
	forward: { local: null, remote: 8080, address: '127.0.0.1' },
	drain: { ignoreDaemonsets: true, deleteEmptydirData: false, force: false, gracePeriodSeconds: null },
	images: [],
	memory: '',
	resources: [],
	resourceProblems: false
};
const allDiffs: DiffSettings = { scale: true, restart: true, image: true, labels: true };
const noDiffs: DiffSettings = { scale: false, restart: false, image: false, labels: false };

describe('action dialog texts', () => {
	it('names the action and its button', () => {
		const scale: ActionDialogSpec = { kind: 'scale', resource: deployments, target: web };
		expect(dialogTitle(scale)).toBe('Scale Deployment');
		expect(confirmLabel(scale, null)).toBe('Scale');
		const autoSync: ActionDialogSpec = { kind: 'gitops', resource: deployments, target: web, action: 'autoSync' };
		expect(confirmLabel(autoSync, { on: true, options: [] })).toBe('Turn off');
		expect(confirmLabel(autoSync, { on: false, options: [] })).toBe('Turn on');
		expect(dialogTitle({ kind: 'suspend', resource: deployments, target: web, suspend: false })).toBe('Resume CronJob');
	});
});

describe('confirming destructive actions', () => {
	const one: ActionDialogSpec = { kind: 'delete', resource: deployments, targets: [web] };
	const two: ActionDialogSpec = { kind: 'delete', resource: deployments, targets: [web, api] };
	const drain: ActionDialogSpec = { kind: 'drain', resource: deployments, target: { ...web, namespace: null, name: 'node-1' } };

	it('knows which actions destroy something', () => {
		expect(isDangerous(one, null)).toBe(true);
		expect(isDangerous({ kind: 'scale', resource: deployments, target: web }, null)).toBe(false);
		// Once a drain has run, the button only closes its report.
		expect(isDangerous(drain, null)).toBe(true);
		expect(isDangerous(drain, { evicted: [], failed: [], skipped: [] } as DrainReport)).toBe(false);
	});

	it('asks for a word only where the button is not enough', () => {
		expect(confirmWord(one, true, false, 'dev')).toBeNull();
		expect(confirmWord(two, true, false, 'dev')).toBe('confirm');
		expect(confirmWord(one, true, true, 'prod')).toBe('web');
		expect(confirmWord(two, true, true, 'prod')).toBe('prod');
		expect(confirmWord(drain, true, true, 'prod')).toBe('node-1');
		expect(confirmWord(one, false, true, 'prod')).toBeNull();
	});

	it('finds the namespaces an action reaches', () => {
		expect(reachedNamespaces(two)).toEqual(['shop', 'shop']);
		// Deleting a namespace reaches everything in it.
		expect(reachedNamespaces({ kind: 'delete', resource: namespaces, targets: [shop] })).toEqual(['shop']);
		expect(reachedNamespaces({ kind: 'restart', resource: deployments, target: web })).toEqual(['shop']);
	});
});

describe('labels and annotations as lines', () => {
	it('reads and writes key=value lines', () => {
		const map = { app: 'web', 'kubectl.kubernetes.io/last-applied-configuration': '{}', tier: 'a=b' };
		expect(toLines(map)).toBe('app=web\ntier=a=b');
		expect(fromLines(' app = web \n\ntier=a=b')).toEqual({ app: 'web', tier: 'a=b' });
		expect(() => fromLines('=value')).toThrow('Expected key=value');
	});

	it('patches what changed and removes what is gone', () => {
		expect(mapPatch({ a: '1', b: '2', 'kubectl.kubernetes.io/last-applied-configuration': '{}' }, { a: '1', c: '3' })).toEqual({ b: null, c: '3' });
		const original = { labels: { app: 'web' }, annotations: {} };
		expect(metadataPatch(original, 'app=api', '')).toEqual({ labels: { app: 'api' }, annotations: {} });
		expect(metadataPatch(original, 'broken', '')).toBeNull();
	});
});

describe('changed images', () => {
	it('keeps the containers whose image was changed', () => {
		const images = [
			{ name: 'web', image: 'nginx:1.26', init: false },
			{ name: 'sidecar', image: 'envoy:1', init: false }
		];
		expect(changedImages(images, { web: ' nginx:1.27 ', sidecar: 'envoy:1' })).toEqual([{ name: 'web', image: 'nginx:1.27', init: false }]);
		expect(changedImages(null, {})).toEqual([]);
	});
});

describe('dry runs and commands', () => {
	const scale: ActionDialogSpec = { kind: 'scale', resource: deployments, target: web };

	it('dry-runs a change when the settings ask for it', () => {
		expect(previewChange(scale, allDiffs, fields)).toEqual({ type: 'scale', replicas: 3 });
		expect(previewChange(scale, noDiffs, fields)).toBeNull();
		expect(previewChange(scale, allDiffs, { ...fields, replicas: -1 })).toBeNull();
		// A new memory limit replaces the pods, so its diff always shows.
		const memory: ActionDialogSpec = {
			kind: 'memory',
			resource: deployments,
			target: web,
			fix: { container: 'web', limit: '128Mi', request: null, suggested: '256Mi' }
		} as ActionDialogSpec;
		expect(previewChange(memory, noDiffs, { ...fields, memory: '256Mi' })).toEqual({ type: 'memory', container: 'web', memory: '256Mi' });
	});

	it('writes the kubectl command as the fields say', () => {
		expect(actionCommand(scale, 'prod', fields)).toBe('kubectl --context prod -n shop scale deployments.apps/web --replicas=3');
		expect(actionCommand({ kind: 'benchmark', forward: {} as never }, 'prod', fields)).toBeNull();
	});
});

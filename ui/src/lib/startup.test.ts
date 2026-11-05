import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_VIEWS, keepViews, lastContext, rememberContext, saveView, savedView, setStartSetting, startContext, startSetting } from './startup';

const contexts = [
	{ name: 'dev', isDefault: false },
	{ name: 'prod', isDefault: true },
	{ name: 'staging', isDefault: false }
];

const view = (page: string, namespace = 'all') => ({ page, resource: { group: 'apps', version: 'v1', plural: 'deployments' }, namespace, xray: null });

beforeEach(() => {
	const items = new Map<string, string>();
	vi.stubGlobal('localStorage', {
		getItem: (key: string) => items.get(key) ?? null,
		setItem: (key: string, value: string) => void items.set(key, String(value)),
		removeItem: (key: string) => void items.delete(key)
	});
});

describe('the context on start', () => {
	it('is the one used last while the kubeconfig names the same current context', () => {
		expect(startContext(contexts, { context: 'dev', kubeconfig: 'prod' }, 'last')).toBe('dev');
	});

	it("follows the kubeconfig when its current context changed since", () => {
		expect(startContext(contexts, { context: 'dev', kubeconfig: 'staging' }, 'last')).toBe('prod');
	});

	it("is the kubeconfig's when the setting says so, or the last one is gone", () => {
		expect(startContext(contexts, { context: 'dev', kubeconfig: 'prod' }, 'kubeconfig')).toBe('prod');
		expect(startContext(contexts, { context: 'old', kubeconfig: 'prod' }, 'last')).toBe('prod');
		expect(startContext(contexts, null, 'last')).toBe('prod');
	});

	it('is the one an address names, if it exists', () => {
		expect(startContext(contexts, { context: 'dev', kubeconfig: 'prod' }, 'last', 'staging')).toBe('staging');
		expect(startContext(contexts, { context: 'dev', kubeconfig: 'prod' }, 'last', 'gone')).toBe('dev');
	});

	it('is the first without a current context, and none without contexts', () => {
		const none = contexts.map((c) => ({ ...c, isDefault: false }));
		expect(startContext(none, null, 'last')).toBe('dev');
		expect(startContext(none, { context: 'staging', kubeconfig: null }, 'last')).toBe('staging');
		expect(startContext([], null, 'last')).toBeNull();
	});

	it('is remembered with the setting', () => {
		expect(lastContext()).toBeNull();
		rememberContext('dev', 'prod');
		expect(lastContext()).toEqual({ context: 'dev', kubeconfig: 'prod' });
		expect(startSetting()).toBe('last');
		setStartSetting('kubeconfig');
		expect(startSetting()).toBe('kubeconfig');
		setStartSetting('last');
		expect(startSetting()).toBe('last');
	});

	it('reads broken storage as nothing', () => {
		localStorage.setItem('kubri.lastContext', '{nope');
		localStorage.setItem('kubri.views', '[1]');
		expect(lastContext()).toBeNull();
		expect(savedView('dev')).toBeNull();
	});

	it('works without storage', () => {
		vi.stubGlobal('localStorage', undefined);
		rememberContext('dev', 'prod');
		expect(lastContext()).toBeNull();
		expect(startSetting()).toBe('last');
	});
});

describe('views kept across restarts', () => {
	it('gives back the view of each context', () => {
		saveView('dev', view('resources', 'web'));
		saveView('prod', view('overview'));
		expect(savedView('dev')).toEqual(view('resources', 'web'));
		expect(savedView('prod')?.page).toBe('overview');
		expect(savedView('staging')).toBeNull();
	});

	it('keeps only the most recent contexts', () => {
		for (let i = 0; i <= MAX_VIEWS; i++) saveView(`c${i}`, view('resources'), i);
		expect(savedView('c0')).toBeNull();
		expect(savedView('c1')).not.toBeNull();
		expect(savedView(`c${MAX_VIEWS}`)).not.toBeNull();
	});

	it('forgets contexts that are gone', () => {
		saveView('dev', view('resources'));
		saveView('old', view('resources'));
		keepViews(['dev', 'prod']);
		expect(savedView('old')).toBeNull();
		expect(savedView('dev')).not.toBeNull();
	});
});

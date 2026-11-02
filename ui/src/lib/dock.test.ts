import { beforeEach, describe, expect, it } from 'vitest';
import { contextOf, dock } from './dock.svelte';

const pods = { group: '', version: 'v1', plural: 'pods', kind: 'Pod', namespaced: true, verbs: [] } as never;
const at = (name: string, context = 'a') => ({ resource: pods, target: { ...ref(name), context } });
const ref = (name: string) => ({ resource: { group: '', version: 'v1', plural: 'pods' }, namespace: 'shop', name });

beforeEach(() => {
	for (const s of [...dock.sessions]) dock.close(s.id);
	dock.minimized = false;
});

describe('workspace', () => {
	it('shows the open tab again for the same logs, and opens a new tab for each shell', () => {
		dock.add({ kind: 'logs', ...at('api') });
		dock.add({ kind: 'logs', ...at('api') });
		dock.add({ kind: 'logs', ...at('api'), previous: true });
		dock.add({ kind: 'shell', ...at('api') });
		dock.add({ kind: 'shell', ...at('api') });
		expect(dock.sessions.map((s) => s.kind)).toEqual(['logs', 'logs', 'shell', 'shell']);
		expect(dock.active).toBe(dock.sessions[3].id);
	});

	it("shows a context's open local terminal again, and opens another when asked", () => {
		expect(dock.local('a', 'shop')).toBe(true);
		dock.add({ kind: 'logs', ...at('api') });
		expect(dock.local('a', 'web')).toBe(false);
		expect(dock.active).toBe(dock.sessions[0].id);
		expect(dock.focused).toBe(dock.sessions[0].id);
		expect(dock.local('b', null)).toBe(true);
		expect(dock.local('a', 'web', true)).toBe(true);
		expect(dock.sessions.filter((s) => s.kind === 'local').map((s) => [contextOf(s), s.kind === 'local' && s.namespace])).toEqual([
			['a', 'shop'],
			['b', null],
			['a', 'web']
		]);
		expect(dock.local('a', null)).toBe(false);
		expect(dock.active).toBe(dock.sessions[3].id);
		expect(dock.keepContexts(['b'])).toBe(3);
	});

	it('splits, picks tabs into the pane last used, and closes into the neighbour', () => {
		dock.add({ kind: 'logs', ...at('one') });
		dock.add({ kind: 'logs', ...at('two') });
		dock.add({ kind: 'logs', ...at('three') });
		const [one, two, three] = dock.sessions.map((s) => s.id);
		dock.toggleSplit();
		expect([dock.active, dock.beside]).toEqual([three, two]);
		dock.last = two;
		dock.show(one);
		expect([dock.active, dock.beside]).toEqual([three, one]);
		dock.close(three);
		expect([dock.active, dock.beside]).toEqual([one, null]);
		dock.close(one);
		expect(dock.active).toBe(two);
	});

	it('opens from the task bar and closes the tabs of contexts that are gone', () => {
		dock.add({ kind: 'logs', ...at('api', 'a') });
		dock.add({ kind: 'shell', ...at('api', 'b') });
		dock.minimized = true;
		expect(dock.open).toBe(false);
		dock.show(dock.sessions[0].id);
		expect(dock.open).toBe(true);
		expect(dock.keepContexts(['a', 'b'])).toBe(0);
		expect(dock.keepContexts(['b', 'c'])).toBe(1);
		expect(dock.sessions.map(contextOf)).toEqual(['b']);
	});
});

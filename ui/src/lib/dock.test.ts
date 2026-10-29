import { beforeEach, describe, expect, it } from 'vitest';
import { dock } from './dock.svelte';

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

	it('opens from the task bar and closes the tabs of another context', () => {
		dock.add({ kind: 'logs', ...at('api', 'a') });
		dock.add({ kind: 'shell', ...at('api', 'b') });
		dock.minimized = true;
		expect(dock.open).toBe(false);
		dock.show(dock.sessions[0].id);
		expect(dock.open).toBe(true);
		expect(dock.keepContext('b')).toBe(1);
		expect(dock.sessions.map((s) => s.target.context)).toEqual(['b']);
	});
});

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { store, stored, storedList } from './storage';

beforeEach(() => {
	const items = new Map<string, string>();
	vi.stubGlobal('localStorage', {
		getItem: (key: string) => items.get(key) ?? null,
		setItem: (key: string, value: string) => void items.set(key, String(value)),
		removeItem: (key: string) => void items.delete(key)
	});
});

describe('choices kept in the browser', () => {
	it('are read back, and null forgets them', () => {
		expect(store('k', 'v')).toBe(true);
		expect(stored('k')).toBe('v');
		store('k', null);
		expect(stored('k')).toBeNull();
	});

	it('read as lists only when they are JSON arrays', () => {
		store('list', '[1,2]');
		expect(storedList('list')).toEqual([1, 2]);
		store('list', '{nope');
		expect(storedList('list')).toEqual([]);
		store('list', '{"a":1}');
		expect(storedList('list')).toEqual([]);
	});

	it('do nothing without storage', () => {
		vi.stubGlobal('localStorage', undefined);
		expect(stored('k')).toBeNull();
		expect(store('k', 'v')).toBe(false);
		expect(storedList('k')).toEqual([]);
	});
});

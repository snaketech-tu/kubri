import { describe, expect, it } from 'vitest';
import { Places, switcherOrder } from './places';
import { suggest } from './commands';

const place = (view: string) => ({ view, history: [view], historyAt: 0, filter: '', cursor: null });

describe('places per context', () => {
	it('gives back what a context showed once, and forgets contexts that are gone', () => {
		const places = new Places<string>();
		places.save('a', place('pods'));
		places.save('b', place('nodes'));
		expect(places.take('a')?.view).toBe('pods');
		expect(places.take('a')).toBeUndefined();
		places.keepOnly(['c']);
		expect(places.take('b')).toBeUndefined();
	});

	it('puts the previous context first only while the search is empty', () => {
		const contexts = [{ name: 'a' }, { name: 'b' }, { name: 'c' }];
		const names = (list: { name: string }[]) => list.map((c) => c.name);
		expect(names(switcherOrder(contexts, 'c', 'a', ''))).toEqual(['c', 'a', 'b']);
		expect(names(switcherOrder(contexts, 'c', 'a', 'x'))).toEqual(['a', 'b', 'c']);
		expect(names(switcherOrder(contexts, null, 'a', ''))).toEqual(['a', 'b', 'c']);
		expect(names(switcherOrder(contexts, 'a', 'a', ''))).toEqual(['a', 'b', 'c']);
	});

	it('offers `ctx -` for the previous context', () => {
		expect(suggest('ctx -', [], [], [])).toEqual([
			{ label: 'ctx -', detail: 'The previous context', action: { type: 'context', name: '-' } }
		]);
	});
});

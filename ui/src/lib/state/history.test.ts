import { describe, expect, it } from 'vitest';
import { History } from './history.svelte';

const same = (a: string, b: string) => a === b;

describe('History', () => {
	it('goes back and forth', () => {
		const history = new History(same);
		for (const view of ['a', 'b', 'c']) history.push(view);
		expect(history.canGoForward).toBe(false);
		expect(history.step(-1)).toBe('b');
		expect(history.step(-1)).toBe('a');
		expect(history.canGoBack).toBe(false);
		expect(history.step(-1)).toBeNull();
		expect(history.step(1)).toBe('b');
	});

	it('drops the places ahead when a new one is shown', () => {
		const history = new History(same);
		for (const view of ['a', 'b', 'c']) history.push(view);
		history.step(-1);
		history.push('d');
		expect(history.entries).toEqual(['a', 'b', 'd']);
		expect(history.canGoForward).toBe(false);
	});

	it('keeps the same place once', () => {
		const history = new History(same);
		history.push('a');
		history.push('a');
		expect(history.entries).toEqual(['a']);
	});

	it('keeps the last 50 places', () => {
		const history = new History(same);
		for (let i = 0; i < 60; i++) history.push(String(i));
		expect(history.entries).toHaveLength(50);
		expect(history.entries[0]).toBe('10');
		expect(history.at).toBe(49);
	});

	it('toggles between the last two places', () => {
		const history = new History(same);
		expect(history.toggleLast()).toBeNull();
		history.push('a');
		history.push('b');
		expect(history.toggleLast()).toBe('a');
		expect(history.at).toBe(0);
		expect(history.toggleLast()).toBe('b');
		expect(history.at).toBe(1);
	});

	it('forgets the last place when reset', () => {
		const history = new History(same);
		history.push('a');
		history.push('b');
		history.reset(['x', 'y'], 0);
		expect(history.toggleLast()).toBeNull();
		expect(history.step(1)).toBe('y');
	});
});

import { describe, expect, it } from 'vitest';
import { escapeTwice } from './keys';

describe('escapeTwice', () => {
	it('answers on the second Esc in quick succession', () => {
		const twice = escapeTwice(400);
		expect(twice('Escape', 1000)).toBe(false);
		expect(twice('Escape', 1300)).toBe(true);
		// A third starts over.
		expect(twice('Escape', 1400)).toBe(false);
	});

	it('ignores a slow second Esc and Esc after other keys', () => {
		const twice = escapeTwice(400);
		expect(twice('Escape', 0)).toBe(false);
		expect(twice('Escape', 900)).toBe(false);
		expect(twice('a', 1000)).toBe(false);
		expect(twice('Escape', 1100)).toBe(false);
	});
});

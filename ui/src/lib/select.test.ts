import { describe, expect, it } from 'vitest';
import { edge, filterOptions, plainOptions, step, typeahead, type SelectOption } from './select';

const options: SelectOption<string>[] = [
	{ value: 'a', label: 'Apple' },
	{ value: 'b', label: 'Banana', disabled: true },
	{ value: 'c', label: 'Cherry' },
	{ value: 'd', label: 'Apricot' },
	{ value: 'e', label: 'Blueberry' }
];
const all = options.map((_, i) => i);

describe('select options', () => {
	it('shows plain values as they are', () => {
		expect(plainOptions([1, 2])).toEqual([
			{ value: 1, label: '1' },
			{ value: 2, label: '2' }
		]);
	});

	it('steps over disabled options and stops at the ends', () => {
		expect(step(options, all, 0, 1)).toBe(2);
		expect(step(options, all, 2, -1)).toBe(0);
		expect(step(options, all, 0, -1)).toBe(0);
		expect(step(options, all, 4, 1)).toBe(4);
	});

	it('pages as far as it can', () => {
		expect(step(options, all, 0, 10)).toBe(4);
		expect(step(options, all, 4, -10)).toBe(0);
		// A page that lands on a disabled option stops short of it.
		expect(step(options, all, 0, 1)).toBe(2);
		expect(step(options, all, 3, -2)).toBe(2);
	});

	it('starts at an end when nothing is active', () => {
		expect(step(options, all, -1, 1)).toBe(0);
		expect(step(options, all, -1, -1)).toBe(4);
		expect(step(options, [2, 4], 0, 1)).toBe(2);
	});

	it('finds the first and last enabled option', () => {
		expect(edge(options, all, false)).toBe(0);
		expect(edge(options, all, true)).toBe(4);
		expect(edge(options, [1], false)).toBe(-1);
	});

	it('jumps to typed letters like a native select', () => {
		expect(typeahead(options, 'c', 0)).toBe(2);
		// The same letter again cycles through the matches.
		expect(typeahead(options, 'a', 0)).toBe(3);
		expect(typeahead(options, 'aa', 3)).toBe(0);
		// A longer word keeps the current option when it still matches.
		expect(typeahead(options, 'apr', 3)).toBe(3);
		expect(typeahead(options, 'app', 3)).toBe(0);
		// Disabled options are skipped.
		expect(typeahead(options, 'b', 0)).toBe(4);
		expect(typeahead(options, 'x', 0)).toBe(-1);
		expect(typeahead(options, '', 0)).toBe(-1);
	});

	it('filters by every word', () => {
		expect(filterOptions(options, '')).toEqual(all);
		expect(filterOptions(options, 'ap')).toEqual([0, 3]);
		expect(filterOptions(options, 'b rr')).toEqual([4]);
		expect(filterOptions(options, 'zz')).toEqual([]);
	});
});

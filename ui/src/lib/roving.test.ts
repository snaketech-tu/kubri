import { describe, expect, it } from 'vitest';
import { nearestVertically, step } from './roving';

describe('step', () => {
	it('moves along its own axis and wraps around', () => {
		expect(step(0, 3, 'ArrowRight', 'horizontal')).toBe(1);
		expect(step(2, 3, 'ArrowRight', 'horizontal')).toBe(0);
		expect(step(0, 3, 'ArrowLeft', 'horizontal')).toBe(2);
		expect(step(1, 3, 'ArrowDown', 'vertical')).toBe(2);
		expect(step(0, 3, 'ArrowUp', 'vertical')).toBe(2);
		expect(step(1, 3, 'End', 'vertical')).toBe(2);
		expect(step(1, 3, 'Home', 'horizontal')).toBe(0);
	});

	it('leaves the other keys alone', () => {
		expect(step(0, 3, 'ArrowDown', 'horizontal')).toBeNull();
		expect(step(0, 3, 'ArrowRight', 'vertical')).toBeNull();
		expect(step(0, 3, 'Enter', 'vertical')).toBeNull();
		expect(step(0, 0, 'ArrowDown', 'vertical')).toBeNull();
	});
});

describe('nearestVertically', () => {
	// Four tiles in a row, then two wide cards below.
	const boxes = [
		{ left: 0, top: 0, width: 100, height: 80 },
		{ left: 110, top: 0, width: 100, height: 80 },
		{ left: 220, top: 0, width: 100, height: 80 },
		{ left: 330, top: 0, width: 100, height: 80 },
		{ left: 0, top: 100, width: 210, height: 200 },
		{ left: 220, top: 100, width: 210, height: 200 }
	];

	it('goes to the tile below that is closest across', () => {
		expect(nearestVertically(boxes, 0, true)).toBe(4);
		expect(nearestVertically(boxes, 3, true)).toBe(5);
		expect(nearestVertically(boxes, 2, true)).toBe(5);
	});

	it('goes up to the closest tile of the row above', () => {
		expect(nearestVertically(boxes, 5, false)).toBe(2);
		expect(nearestVertically(boxes, 4, false)).toBe(0);
	});

	it('stops at the edges', () => {
		expect(nearestVertically(boxes, 1, false)).toBeNull();
		expect(nearestVertically(boxes, 4, true)).toBeNull();
	});
});

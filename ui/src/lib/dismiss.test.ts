import { describe, expect, it } from 'vitest';
import { outside } from './dismiss';

describe('outside', () => {
	const box = { left: 100, right: 300, top: 50, bottom: 250 } as DOMRect;

	it('tells the backdrop from the dialog', () => {
		expect(outside(box, 99, 100)).toBe(true);
		expect(outside(box, 200, 251)).toBe(true);
		expect(outside(box, 100, 50)).toBe(false);
		expect(outside(box, 200, 150)).toBe(false);
	});
});

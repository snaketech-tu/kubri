import { describe, expect, it } from 'vitest';
import { clampWidth } from './resize';

describe('clampWidth', () => {
	it('keeps a width between the smallest and largest size', () => {
		expect(clampWidth(300, 220, 480)).toBe(300);
		expect(clampWidth(100, 220, 480)).toBe(220);
		expect(clampWidth(900, 220, 480)).toBe(480);
	});

	it('rounds to whole pixels', () => {
		expect(clampWidth(300.6, 220, 480)).toBe(301);
	});

	it('prefers the smallest size when there is no room for it', () => {
		expect(clampWidth(400, 320, 200)).toBe(320);
	});
});

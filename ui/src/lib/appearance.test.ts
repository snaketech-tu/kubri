import { describe, expect, it } from 'vitest';
import { contrast, skinWarning, stepZoom, zoomKey } from './appearance';

const key = (k: string, mods: Partial<KeyboardEvent> = { ctrlKey: true }) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods }) as KeyboardEvent;

describe('zoom', () => {
	it('steps through the levels and stops at the ends', () => {
		expect(stepZoom(1, 1)).toBe(1.1);
		expect(stepZoom(1, -1)).toBe(0.9);
		expect(stepZoom(2, 1)).toBe(2);
		expect(stepZoom(0.67, -1)).toBe(0.67);
		expect(stepZoom(1.75, 0)).toBe(1);
		// A level between steps goes to the next one.
		expect(stepZoom(1.2, 1)).toBe(1.25);
	});

	it('knows the zoom keys', () => {
		expect(zoomKey(key('='))).toBe(1);
		expect(zoomKey(key('+', { metaKey: true }))).toBe(1);
		expect(zoomKey(key('-'))).toBe(-1);
		expect(zoomKey(key('0'))).toBe(0);
		expect(zoomKey(key('-', {}))).toBeNull();
		expect(zoomKey(key('0', { ctrlKey: true, altKey: true }))).toBeNull();
	});
});

describe('contrast', () => {
	it('measures as WCAG does', () => {
		expect(contrast('#000000', '#ffffff')).toBeCloseTo(21);
		expect(contrast('#ffffff', '#ffffff')).toBeCloseTo(1);
		expect(contrast('#767676', '#ffffff')).toBeCloseTo(4.54, 1);
	});

	it('warns about skins that are hard to read', () => {
		expect(skinWarning(null)).toBeNull();
		expect(skinWarning({ vars: { '--bg': '#000000', '--text': '#ffffff' }, scheme: 'dark' })).toBeNull();
		expect(skinWarning({ vars: { '--bg': '#000000', '--text': '#333333' }, scheme: 'dark' })).toMatch(/^Text has a contrast of 1\.\d:1/);
		// Text falls back to Kubri's own, which is light on a dark skin.
		expect(skinWarning({ vars: { '--bg': '#ffffff' }, scheme: 'dark' })).toMatch(/^Text/);
		expect(skinWarning({ vars: { '--bg': '#000000', '--text': '#ffffff', '--bad': '#200000' }, scheme: 'dark' })).toBe(
			'The error color is hard to see on the background'
		);
	});
});

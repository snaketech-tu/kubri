import { describe, expect, it } from 'vitest';
import { actionFor, grouped, hintOf, keyFromLabel, keyLabel, keyOf, type MenuAction } from './menu';

const action = (label: string, keys: string[], group = 'View', more: Partial<MenuAction> = {}): MenuAction => ({
	label,
	keys,
	group,
	run: () => {},
	...more
});

const press = (key: string, mods: Partial<KeyboardEventInit> = {}) => ({ key, ctrlKey: false, metaKey: false, ...mods }) as KeyboardEvent;

describe('keyOf', () => {
	it('names plain keys as typed', () => {
		expect(keyOf(press('l'))).toBe('l');
		expect(keyOf(press('F'))).toBe('F');
		expect(keyOf(press('Delete'))).toBe('Delete');
	});

	it('puts ctrl+ in front for Ctrl and Cmd, in lower case', () => {
		expect(keyOf(press('D', { ctrlKey: true }))).toBe('ctrl+d');
		expect(keyOf(press('l', { metaKey: true }))).toBe('ctrl+l');
	});
});

describe('keyLabel', () => {
	it('writes keys the way the hints do', () => {
		expect(keyLabel('l')).toBe('l');
		expect(keyLabel('F')).toBe('⇧f');
		expect(keyLabel('ctrl+d')).toBe('^d');
		expect(keyLabel('Delete')).toBe('del');
		expect(keyLabel(' ')).toBe('space');
	});
});

describe('actionFor and hintOf', () => {
	const cordon = action('Cordon', ['c'], 'Change');
	const copy = action('Copy name', ['c'], 'Copy');
	const remove = action('Delete…', ['ctrl+d', 'Delete'], 'Delete');
	const drill = action('Show pods', [], 'View', { hint: 'enter' });
	const all = [cordon, copy, remove, drill];

	it('runs the first action with the key', () => {
		expect(actionFor(all, 'c')).toBe(cordon);
		expect(actionFor(all, 'Delete')).toBe(remove);
		expect(actionFor(all, 'x')).toBeUndefined();
	});

	it('hints a key only on the action it runs', () => {
		expect(hintOf(all, cordon)).toBe('c');
		expect(hintOf(all, copy)).toBeNull();
		expect(hintOf(all, remove)).toBe('^d');
		expect(hintOf(all, drill)).toBe('enter');
	});
});

describe('grouped', () => {
	it('keeps groups together, in the order they first show up', () => {
		const a = action('a', [], 'View');
		const b = action('b', [], 'Change');
		const c = action('c', [], 'View');
		expect(grouped([a, b, c])).toEqual([
			['View', [a, c]],
			['Change', [b]]
		]);
	});
});

describe('keyFromLabel', () => {
	it('turns hints back into key presses', () => {
		expect(keyFromLabel(':')).toEqual({ key: ':' });
		expect(keyFromLabel('enter')).toEqual({ key: 'Enter' });
		expect(keyFromLabel('esc')).toEqual({ key: 'Escape' });
		expect(keyFromLabel('^d')).toEqual({ key: 'd', ctrlKey: true });
		expect(keyFromLabel('⇧f')).toEqual({ key: 'F', shiftKey: true });
		expect(keyFromLabel('^space')).toEqual({ key: ' ', ctrlKey: true });
	});

	it('leaves out hints for several keys', () => {
		expect(keyFromLabel('j/k')).toBeNull();
		expect(keyFromLabel('h/l')).toBeNull();
		expect(keyFromLabel('←→')).toBeNull();
	});

	it('round-trips the labels of keyLabel', () => {
		for (const key of ['l', 'F', 'ctrl+d', 'Delete', 'Enter']) {
			const press = keyFromLabel(keyLabel(key))!;
			expect(keyOf({ key: press.key!, ctrlKey: !!press.ctrlKey, metaKey: false } as KeyboardEvent)).toBe(key);
		}
	});
});

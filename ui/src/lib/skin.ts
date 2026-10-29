import type { SkinTheme } from './k9s';

let canvas: CanvasRenderingContext2D | null | undefined;

/** `#rrggbb` for any CSS color, such as the names k9s skins use; null if it is not one. */
export function resolveColor(color: string): string | null {
	canvas ??= document.createElement('canvas').getContext('2d');
	if (!canvas || !CSS.supports('color', color)) return null;
	canvas.fillStyle = '#000000';
	canvas.fillStyle = color;
	const resolved = canvas.fillStyle;
	return /^#[0-9a-f]{6}$/i.test(resolved) ? resolved : null;
}

let applied: string[] = [];

/** Light or dark picked in Kubri; null follows the system. */
export type Scheme = 'light' | 'dark' | null;
const SCHEME_KEY = 'kubri.scheme';
let scheme: Scheme = null;

export function storedScheme(): Scheme {
	try {
		const value = localStorage.getItem(SCHEME_KEY);
		return value === 'light' || value === 'dark' ? value : null;
	} catch {
		return null;
	}
}

/** Puts Kubri's own colors in light or dark; a skin on screen keeps its scheme until it is taken off. */
export function applyScheme(next: Scheme) {
	scheme = next;
	try {
		if (next) localStorage.setItem(SCHEME_KEY, next);
		else localStorage.removeItem(SCHEME_KEY);
	} catch {
		// Lasts until the page is reloaded.
	}
	const root = document.documentElement;
	if (root.classList.contains('skinned')) return;
	root.classList.remove('light', 'dark');
	if (next) root.classList.add(next);
}

/** Puts a skin's colors on the page, or Kubri's own back with null. */
export function applyTheme(theme: SkinTheme | null) {
	const root = document.documentElement;
	for (const name of applied) root.style.removeProperty(name);
	root.classList.remove('skinned', 'light', 'dark');
	applied = [];
	if (!theme) {
		if (scheme) root.classList.add(scheme);
		return;
	}
	for (const [name, value] of Object.entries(theme.vars)) root.style.setProperty(name, value);
	applied = Object.keys(theme.vars);
	root.classList.add('skinned', theme.scheme);
}

const KEY = 'kubri.skin.';

/** The skin picked for a context: undefined follows k9s, null is Kubri's own colors. */
export function storedSkin(context: string): string | null | undefined {
	try {
		const value = localStorage.getItem(KEY + context);
		return value === null ? undefined : value === '' ? null : value;
	} catch {
		return undefined;
	}
}

export function storeSkin(context: string, skin: string | null | undefined) {
	try {
		if (skin === undefined) localStorage.removeItem(KEY + context);
		else localStorage.setItem(KEY + context, skin ?? '');
	} catch {
		// Without storage the choice lasts until the page is reloaded.
	}
}

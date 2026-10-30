import { isTauri } from '@tauri-apps/api/core';
import type { SkinTheme } from './k9s';

/** The zoom levels Ctrl+ and Ctrl− step through, as browsers offer them. */
export const ZOOMS = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
const ZOOM_KEY = 'kubri.zoom';

/** The zoom level kept from the last time; 1 without one. */
export function storedZoom(): number {
	try {
		const value = Number(localStorage.getItem(ZOOM_KEY));
		return ZOOMS.includes(value as (typeof ZOOMS)[number]) ? value : 1;
	} catch {
		return 1;
	}
}

/** The next level up (`1`) or down (`-1`) from `zoom`; `0` goes back to 100%. */
export function stepZoom(zoom: number, step: -1 | 0 | 1): number {
	if (step === 0) return 1;
	const levels = step > 0 ? ZOOMS : [...ZOOMS].reverse();
	return levels.find((z) => (step > 0 ? z > zoom + 0.001 : z < zoom - 0.001)) ?? zoom;
}

/**
 * Zooms the desktop app's window and keeps the level for the next start. In the browser, its own
 * zoom does this and remembers it per site, so Kubri leaves it alone.
 */
export async function applyZoom(zoom: number) {
	if (!isTauri()) return;
	try {
		localStorage.setItem(ZOOM_KEY, String(zoom));
	} catch {
		// Lasts until the app is closed.
	}
	const { getCurrentWebview } = await import('@tauri-apps/api/webview');
	await getCurrentWebview().setZoom(zoom);
}

/** The Ctrl or Cmd key that zooms, if `e` is one: `=` and `+` in, `-` out, `0` back to 100%. */
export function zoomKey(e: KeyboardEvent): -1 | 0 | 1 | null {
	if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
	if (e.key === '=' || e.key === '+') return 1;
	if (e.key === '-') return -1;
	return e.key === '0' ? 0 : null;
}

/** The relative luminance of `#rrggbb`, as WCAG defines it. */
export function luminance(hex: string): number {
	const [r, g, b] = [1, 3, 5].map((i) => {
		const c = parseInt(hex.slice(i, i + 2), 16) / 255;
		return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG contrast ratio of two `#rrggbb` colors, from 1 to 21. */
export function contrast(a: string, b: string): number {
	const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (light + 0.05) / (dark + 0.05);
}

/** Kubri's own background and text, which a skin's colors fall back to. */
const OWN = {
	light: { '--bg': '#f6f7fb', '--text': '#20283d' },
	dark: { '--bg': '#101522', '--text': '#e5eaf5' }
} as const;

/**
 * What makes a skin hard to read in Kubri, or null: text below the 4.5:1 contrast WCAG asks of
 * body text, or status colors below the 3:1 it asks of large text and marks.
 */
export function skinWarning(theme: SkinTheme | null | undefined): string | null {
	if (!theme) return null;
	const bg = theme.vars['--bg'] ?? OWN[theme.scheme]['--bg'];
	const text = theme.vars['--text'] ?? OWN[theme.scheme]['--text'];
	const ratio = contrast(text, bg);
	if (ratio < 4.5) return `Text has a contrast of ${ratio.toFixed(1)}:1 on the background, below the 4.5:1 that reads easily`;
	const faint = (['--accent', '--ok', '--warn', '--bad'] as const).filter((name) => {
		const color = theme.vars[name];
		return color && contrast(color, bg) < 3;
	});
	if (faint.length) {
		const names = faint.map((n) => ({ '--accent': 'links', '--ok': 'ok', '--warn': 'warning', '--bad': 'error' })[n]);
		return `The ${names.join(', ')} ${faint.length === 1 ? 'color is' : 'colors are'} hard to see on the background`;
	}
	return null;
}

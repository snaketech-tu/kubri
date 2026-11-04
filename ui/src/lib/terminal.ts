import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import type { TerminalSession } from './api';
import { fontStack } from './fonts';

/** A dark terminal in both themes: the ANSI colors programs use are made for it. */
export const TERMINAL_BACKGROUND = '#0f131a';

const THEME = {
	background: TERMINAL_BACKGROUND,
	foreground: '#e4e8ef',
	cursor: '#8fb1f5',
	selectionBackground: '#3b4a6680'
};

/** How long a terminal waits for its fonts before it opens with what it has. */
const FONT_WAIT_MS = 1500;

/**
 * Loads the bundled fonts the terminal may use, so that it measures its cells with them and
 * not with a font it shows only until they arrive. Fonts are fetched once; later terminals
 * open at once.
 */
async function loadFonts(fontSize: number, family: string): Promise<void> {
	if (typeof document === 'undefined' || !document.fonts) return;
	// A font name the browser cannot read throws here; the terminal then opens without waiting.
	const load = (font: string, text?: string) => {
		try {
			return document.fonts.load(font, text);
		} catch {
			return Promise.resolve([]);
		}
	};
	const faces = [
		load(`${fontSize}px ${family}`),
		load(`bold ${fontSize}px ${family}`),
		// Only fetched for its symbols, like the arrows and icons of powerline prompts.
		load(`${fontSize}px "Kubri Symbols"`, '\ue0b0')
	];
	await Promise.race([Promise.allSettled(faces), new Promise((done) => setTimeout(done, FONT_WAIT_MS))]);
}

/**
 * Shows a terminal in `element` that follows its size, connects it with `connect`, and returns
 * a function that closes both. `fontFamily` names fonts to try before Kubri's own.
 */
export function mountTerminal(
	element: HTMLElement,
	connect: (term: Terminal) => TerminalSession,
	{ fontSize = 13, fontFamily }: { fontSize?: number; fontFamily?: string } = {}
): () => void {
	const family = fontStack(fontFamily, getComputedStyle(element).getPropertyValue('--mono'));
	let close: (() => void) | null = null;
	let closed = false;
	const open = () => {
		if (closed) return;
		const term = new Terminal({
			fontFamily: family,
			fontSize,
			cursorBlink: true,
			scrollback: 5000,
			theme: THEME
		});
		const fit = new FitAddon();
		term.loadAddon(fit);
		term.open(element);
		fit.fit();
		const session = connect(term);
		const input = term.onData((data) => session.write(data));
		const resized = term.onResize(({ cols, rows }) => session.resize(cols, rows));
		// Panels change size with the window and when they are maximized.
		const observer = new ResizeObserver(() => fit.fit());
		observer.observe(element);
		term.focus();
		close = () => {
			observer.disconnect();
			input.dispose();
			resized.dispose();
			session.close();
			term.dispose();
		};
	};
	loadFonts(fontSize, family).then(open, open);
	return () => {
		closed = true;
		close?.();
	};
}

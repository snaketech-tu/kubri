import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import type { TerminalSession } from './api';

/** A dark terminal in both themes: the ANSI colors programs use are made for it. */
export const TERMINAL_BACKGROUND = '#0f131a';

const THEME = {
	background: TERMINAL_BACKGROUND,
	foreground: '#e4e8ef',
	cursor: '#8fb1f5',
	selectionBackground: '#3b4a6680'
};

/**
 * Shows a terminal in `element` that follows its size, connects it with `connect`, and returns
 * a function that closes both.
 */
export function mountTerminal(element: HTMLElement, connect: (term: Terminal) => TerminalSession): () => void {
	const term = new Terminal({
		fontFamily: getComputedStyle(element).getPropertyValue('--mono') || 'monospace',
		fontSize: 13,
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
	return () => {
		observer.disconnect();
		input.dispose();
		resized.dispose();
		session.close();
		term.dispose();
	};
}

/** The element the last Tab moved to, while it keeps the focus. */
let tabbedTo: EventTarget | null = null;
let tabbing = false;
if (typeof window !== 'undefined') {
	window.addEventListener('keydown', (e) => (tabbing = e.key === 'Tab'), true);
	window.addEventListener('pointerdown', () => (tabbing = false), true);
	window.addEventListener('focusin', (e) => {
		tabbedTo = tabbing ? e.target : null;
		tabbing = false;
	});
}

/** Focuses a button as if reached with Tab, so Enter and Space press it. */
export function focusForKeys(el: HTMLElement) {
	el.focus();
	tabbedTo = el;
}

/**
 * Whether a key press belongs to the focused form control rather than to the shortcuts.
 * Checkboxes only keep the keys that toggle them, so shortcuts still work after a click. Buttons of a
 * toolbar or menu keep Enter and Space, for whoever reaches them with Tab. So does any button or link
 * reached with Tab; one focused by a click, or again when a dialog closes, leaves them to the shortcuts.
 */
export function forControl(e: KeyboardEvent): boolean {
	const el = e.target as HTMLElement;
	if (el.matches?.('input[type="checkbox"], input[type="radio"]')) return e.key === ' ' || e.key === 'Enter';
	if (el.matches?.('[role="toolbar"] button, [role="menu"] button')) return e.key === ' ' || e.key === 'Enter';
	if (el === tabbedTo && el.matches?.('button:not([role="combobox"]), a[href], summary')) return e.key === ' ' || e.key === 'Enter';
	// A select box is a button: it keeps the keys that open it and pick by typing, not : or ?.
	if (el.matches?.('button[role="combobox"]')) return /^([a-z0-9]|Arrow(Up|Down)|Enter| )$/i.test(e.key);
	return !!el.closest?.('input, textarea, select, [role="combobox"], .cm-editor, [contenteditable="true"]');
}

/**
 * Tells when Esc comes twice within `ms`: the way out of a terminal, which takes a single Esc
 * itself, as vim needs it. Call the result with each key pressed and when.
 */
export function escapeTwice(ms = 400): (key: string, at: number) => boolean {
	let last = -Infinity;
	return (key, at) => {
		if (key !== 'Escape') {
			last = -Infinity;
			return false;
		}
		if (at - last <= ms) {
			last = -Infinity;
			return true;
		}
		last = at;
		return false;
	};
}

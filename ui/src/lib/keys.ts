/**
 * Whether a key press belongs to the focused form control rather than to the shortcuts.
 * Checkboxes only keep the keys that toggle them, so shortcuts still work after a click. Buttons of a
 * toolbar or menu keep Enter and Space, for whoever reaches them with Tab.
 */
export function forControl(e: KeyboardEvent): boolean {
	const el = e.target as HTMLElement;
	if (el.matches?.('input[type="checkbox"], input[type="radio"]')) return e.key === ' ' || e.key === 'Enter';
	if (el.matches?.('[role="toolbar"] button, [role="menu"] button')) return e.key === ' ' || e.key === 'Enter';
	// A select box is a button: it keeps the keys that open it and pick by typing, not : or ?.
	if (el.matches?.('button[role="combobox"]')) return /^([a-z0-9]|Arrow(Up|Down)|Enter| )$/i.test(e.key);
	return !!el.closest?.('input, textarea, select, [role="combobox"], .cm-editor, [contenteditable="true"]');
}

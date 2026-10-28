/**
 * Whether a key press belongs to the focused form control rather than to the shortcuts.
 * Checkboxes only keep the keys that toggle them, so shortcuts still work after a click.
 */
export function forControl(e: KeyboardEvent): boolean {
	const el = e.target as HTMLElement;
	if (el.matches?.('input[type="checkbox"], input[type="radio"]')) return e.key === ' ' || e.key === 'Enter';
	return !!el.closest?.('input, textarea, select, .cm-editor, [contenteditable="true"]');
}

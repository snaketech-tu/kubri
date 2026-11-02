/** True when the pointer is outside `box`, on the backdrop of a modal dialog. */
export function outside(box: DOMRect, x: number, y: number): boolean {
	return x < box.left || x > box.right || y < box.top || y > box.bottom;
}

/**
 * Closes a modal `<dialog>` when its backdrop is clicked, as Esc does: it fires the dialog's
 * `cancel` event, so each dialog decides what closing means, and one that is busy stays open.
 * A press that starts inside the dialog, as when selecting text, does not count.
 * Use it as `{@attach lightDismiss}` on the dialog.
 */
export function lightDismiss(dialog: HTMLDialogElement) {
	let pressedOutside = false;
	const onBackdrop = (e: MouseEvent) => e.target === dialog && outside(dialog.getBoundingClientRect(), e.clientX, e.clientY);
	const down = (e: PointerEvent) => {
		pressedOutside = onBackdrop(e);
	};
	const click = (e: MouseEvent) => {
		const dismiss = pressedOutside && onBackdrop(e);
		pressedOutside = false;
		if (!dismiss || !dialog.open) return;
		if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) dialog.close();
	};
	dialog.addEventListener('pointerdown', down);
	dialog.addEventListener('click', click);
	return () => {
		dialog.removeEventListener('pointerdown', down);
		dialog.removeEventListener('click', click);
	};
}

/**
 * Closes a `<details>` used as a drop-down menu when the pointer is pressed anywhere outside it,
 * or when Esc is pressed inside it, which then hands focus back to its summary. A list opened
 * from a select box inside it counts as inside, and Esc closes that list first.
 * Use it as `{@attach dropdown}` on the details element.
 */
export function dropdown(details: HTMLDetailsElement) {
	const away = (e: PointerEvent) => {
		if (details.open && !details.contains(e.target as Node)) details.open = false;
	};
	const keydown = (e: KeyboardEvent) => {
		if (e.key !== 'Escape' || !details.open) return;
		// An open list inside handles this Esc itself.
		if ((e.target as Element).closest?.('[aria-expanded="true"]')) return;
		e.preventDefault();
		e.stopPropagation();
		details.open = false;
		details.querySelector('summary')?.focus();
	};
	document.addEventListener('pointerdown', away, true);
	details.addEventListener('keydown', keydown);
	return () => {
		document.removeEventListener('pointerdown', away, true);
		details.removeEventListener('keydown', keydown);
	};
}

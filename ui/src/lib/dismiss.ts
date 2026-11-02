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

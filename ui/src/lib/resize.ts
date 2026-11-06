/** Keeps a dragged width between its smallest and largest size. */
export function clampWidth(width: number, min: number, max: number): number {
	return Math.round(Math.max(min, Math.min(Math.max(min, max), width)));
}

/**
 * Follows a drag that started on a resize handle, calling `move` with the pointer's x position
 * until the button is let go.
 */
export function followDrag(e: PointerEvent, move: (x: number) => void): void {
	if (e.button !== 0) return;
	e.preventDefault();
	const handle = e.currentTarget as HTMLElement;
	handle.setPointerCapture(e.pointerId);
	const onmove = (ev: PointerEvent) => move(ev.clientX);
	const up = () => {
		handle.removeEventListener('pointermove', onmove);
		handle.removeEventListener('pointerup', up);
		handle.removeEventListener('pointercancel', up);
	};
	handle.addEventListener('pointermove', onmove);
	handle.addEventListener('pointerup', up);
	handle.addEventListener('pointercancel', up);
}

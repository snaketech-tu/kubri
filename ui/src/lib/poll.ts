/**
 * Asking the cluster again on a timer, gently: not while nobody looks, and once more right after a
 * change made in Kubri, so a slow timer does not make the page feel slow.
 */

/** What tells whether the page is shown; the document, or a stand-in in tests. */
export interface Visibility extends EventTarget {
	readonly hidden: boolean;
}

/** How long after a change Kubri asks again: the cluster needs a moment to act on it. */
export const AFTER_CHANGE_MS = 1500;

const changes = new EventTarget();

/** Says that Kubri just changed something in the cluster, so open pages ask again soon. */
export function changedSomething() {
	changes.dispatchEvent(new Event('changed'));
}

/**
 * Runs `load` every `ms` while the page is shown. A page hidden for longer than `ms` (another tab,
 * a minimized window) asks nothing, and asks once as soon as it shows again. After
 * `changedSomething()` it asks again shortly. Returns the function that stops it.
 * `load` does not run at once; call it yourself where the page first loads.
 */
export function every(
	ms: number,
	load: () => unknown,
	page: Visibility | null = typeof document === 'undefined' ? null : document
): () => void {
	let timer: ReturnType<typeof setTimeout> | undefined;
	/** A turn came while the page was hidden. */
	let missed = false;
	const run = () => {
		clearTimeout(timer);
		timer = setTimeout(turn, ms);
		load();
	};
	const turn = () => {
		if (page?.hidden) missed = true;
		else run();
	};
	const shown = () => {
		if (page?.hidden || !missed) return;
		missed = false;
		run();
	};
	const changed = () => {
		clearTimeout(timer);
		timer = setTimeout(turn, AFTER_CHANGE_MS);
	};
	timer = setTimeout(turn, ms);
	page?.addEventListener('visibilitychange', shown);
	changes.addEventListener('changed', changed);
	return () => {
		clearTimeout(timer);
		page?.removeEventListener('visibilitychange', shown);
		changes.removeEventListener('changed', changed);
	};
}

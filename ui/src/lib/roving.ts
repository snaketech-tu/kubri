import type { Attachment } from 'svelte/attachments';
import { focusForKeys, reachedByKeys } from './keys';

/** Which arrows move through a group: across a row of tabs or buttons, or down a list. */
export type Orientation = 'horizontal' | 'vertical';

/**
 * Where an arrow key goes in a group of `count` items from `index`, or null for keys the group
 * leaves alone. The ends wrap around, as tabs do; Home and End go to the first and last.
 */
export function step(index: number, count: number, key: string, orientation: Orientation): number | null {
	if (!count) return null;
	const [back, forth] = orientation === 'horizontal' ? ['ArrowLeft', 'ArrowRight'] : ['ArrowUp', 'ArrowDown'];
	if (key === back) return (index - 1 + count) % count;
	if (key === forth) return (index + 1) % count;
	if (key === 'Home') return 0;
	if (key === 'End') return count - 1;
	return null;
}

/**
 * Arrow keys through a group, like Tab: a row of tabs, a toolbar, the sidebar. The group is one
 * stop for Tab, at its current item (the selected tab, the current page) or else the first.
 * The arrows only work once the group was reached with the keys: after a click they stay with
 * the list or page, as before. With `activate`, moving also presses the item, as tabs do.
 */
export function roving({
	items,
	orientation,
	activate = false
}: {
	/** The items, as a selector within the group. */
	items: string;
	orientation: Orientation;
	activate?: boolean;
}): Attachment<HTMLElement> {
	return (group) => {
		const all = () => [...group.querySelectorAll<HTMLElement>(items)].filter((el) => !el.hasAttribute('disabled') && el.offsetParent !== null);
		/** Keeps one item a Tab stop: the focused one, else the current one, else the first. */
		const sync = () => {
			const list = all();
			const focused = list.find((el) => el === document.activeElement);
			const current =
				focused ??
				list.find((el) => el.getAttribute('aria-selected') === 'true' || (el.hasAttribute('aria-current') && el.getAttribute('aria-current') !== 'false')) ??
				list[0];
			for (const el of group.querySelectorAll<HTMLElement>(items)) {
				const want = el === current ? '0' : '-1';
				if (el.getAttribute('tabindex') !== want) el.setAttribute('tabindex', want);
			}
		};
		const onkeydown = (e: KeyboardEvent) => {
			if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
			const list = all();
			const at = list.indexOf(e.target as HTMLElement);
			if (at === -1 || !reachedByKeys(list[at])) return;
			const next = step(at, list.length, e.key, orientation);
			if (next === null) return;
			e.preventDefault();
			e.stopPropagation();
			focusForKeys(list[next]);
			if (activate) list[next].click();
			sync();
		};
		sync();
		const watch = new MutationObserver(sync);
		watch.observe(group, { subtree: true, childList: true, attributes: true, attributeFilter: ['aria-selected', 'aria-current', 'disabled'] });
		group.addEventListener('keydown', onkeydown);
		group.addEventListener('focusin', sync);
		return () => {
			watch.disconnect();
			group.removeEventListener('keydown', onkeydown);
			group.removeEventListener('focusin', sync);
		};
	};
}

const FOCUSABLE = 'button:not([disabled]), a[href], input:not([disabled]), [tabindex]:not([tabindex="-1"])';
/** What Tab can reach in an item: the item itself, or else what it holds. */
const stops = (item: Element) => (item.matches(FOCUSABLE) ? [item] : [...item.querySelectorAll(FOCUSABLE)]) as HTMLElement[];

/**
 * Scrolls the item the keys just picked, such as a row, into view. When the focus is on another
 * item of the same group, it moves along, to the same control in the new item, so Tab and the
 * arrows never mark two items at once.
 */
export function follow(item: HTMLElement | null | undefined, group: HTMLElement | null | undefined) {
	if (!item) return;
	item.scrollIntoView({ block: 'nearest' });
	const focused = document.activeElement;
	if (!group || !(focused instanceof HTMLElement) || !group.contains(focused) || item.contains(focused)) return;
	const before = focused.closest(item.tagName);
	const at = before && group.contains(before) ? stops(before).indexOf(focused) : -1;
	if (at === -1) return;
	const all = stops(item);
	const next = all[at] ?? all[0];
	if (!next) return;
	if (reachedByKeys(focused)) focusForKeys(next);
	else next.focus({ preventScroll: true });
}

/** Where a tile sits on the page. */
export interface Box {
	left: number;
	top: number;
	width: number;
	height: number;
}

/**
 * The tile below (`down`) or above the tile at `from`, in a grid whose rows may differ: the
 * nearest row that way, and in it the tile closest across. Null at the top or bottom edge.
 */
export function nearestVertically(boxes: Box[], from: number, down: boolean): number | null {
	const here = boxes[from];
	if (!here) return null;
	const middle = (b: Box) => b.left + b.width / 2;
	const beyond = boxes
		.map((b, i) => ({ b, i }))
		.filter(({ b }) => (down ? b.top >= here.top + here.height / 2 : b.top + b.height <= here.top + here.height / 2));
	if (!beyond.length) return null;
	const rowTop = down ? Math.min(...beyond.map(({ b }) => b.top)) : Math.max(...beyond.map(({ b }) => b.top));
	const row = beyond.filter(({ b }) => Math.abs(b.top - rowTop) < 4);
	row.sort((x, y) => Math.abs(middle(x.b) - middle(here)) - Math.abs(middle(y.b) - middle(here)));
	return row[0].i;
}

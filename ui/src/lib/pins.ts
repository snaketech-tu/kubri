import type { RecentObject } from './search';

const KEY = 'kubri.pins';
const MAX = 30;

/**
 * Something kept in the sidebar: a kind (empty name) or one object, such as a namespace.
 * The shape is the one of recent objects, so both open the same way.
 */
export type Pin = RecentObject;

export function loadPins(): Pin[] {
	try {
		const parsed = JSON.parse(localStorage.getItem(KEY) ?? '[]');
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
}

export function samePin(a: Pin, b: Pin): boolean {
	return (
		a.context === b.context &&
		a.group === b.group &&
		a.plural === b.plural &&
		(a.namespace ?? null) === (b.namespace ?? null) &&
		a.name === b.name
	);
}

/** The pins with `pin` added, or removed if it was there. */
export function toggled(pins: Pin[], pin: Pin): Pin[] {
	return pins.some((p) => samePin(p, pin)) ? pins.filter((p) => !samePin(p, pin)) : [...pins, pin].slice(-MAX);
}

export function savePins(pins: Pin[]) {
	try {
		localStorage.setItem(KEY, JSON.stringify(pins));
	} catch {
		// Private windows may refuse; the pins then last until Kubri closes.
	}
}

import type { RecentObject } from './search';
import { store, storedList } from './storage';

const KEY = 'kubri.pins';
const MAX = 30;

/**
 * Something kept in the sidebar: a kind (empty name) or one object, such as a namespace.
 * The shape is the one of recent objects, so both open the same way.
 */
export type Pin = RecentObject;

export function loadPins(): Pin[] {
	return storedList<Pin>(KEY);
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
	store(KEY, JSON.stringify(pins));
}

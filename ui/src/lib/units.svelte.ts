/**
 * How amounts of CPU and memory read: in plain units (0.25 cores, 512 MiB), or as kubectl writes
 * them (250m, 512Mi). Kept per browser or app, like the zoom.
 */
import { store, stored } from './storage';

const KEY = 'kubri.units';

export const units = $state({ kubectl: stored(KEY) === 'kubectl' });

export function setKubectlUnits(on: boolean) {
	units.kubectl = on;
	store(KEY, on ? 'kubectl' : null);
}

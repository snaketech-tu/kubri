/**
 * How amounts of CPU and memory read: in plain units (0.25 cores, 512 MiB), or as kubectl writes
 * them (250m, 512Mi). Kept per browser or app, like the zoom.
 */
const KEY = 'kubri.units';

function load(): boolean {
	try {
		return localStorage.getItem(KEY) === 'kubectl';
	} catch {
		return false;
	}
}

export const units = $state({ kubectl: load() });

export function setKubectlUnits(on: boolean) {
	units.kubectl = on;
	try {
		if (on) localStorage.setItem(KEY, 'kubectl');
		else localStorage.removeItem(KEY);
	} catch {
		// Lasts until Kubri is closed.
	}
}

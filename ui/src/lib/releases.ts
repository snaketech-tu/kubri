/** Whether Kubri asks GitHub for new releases, and which one the user waved away. */
const CHECKS = 'kubri.releaseChecks';
const DISMISSED = 'kubri.releaseDismissed';

function read(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function write(key: string, value: string) {
	try {
		localStorage.setItem(key, value);
	} catch {
		// Lasts until Kubri is closed.
	}
}

/** On unless turned off. */
export const releaseChecks = () => read(CHECKS) !== 'off';
export const setReleaseChecks = (on: boolean) => write(CHECKS, on ? 'on' : 'off');
export const dismissedRelease = () => read(DISMISSED);
export const dismissRelease = (version: string) => write(DISMISSED, version);

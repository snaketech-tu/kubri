import { store, stored } from './storage';

/** Whether Kubri asks GitHub for new releases, and which one the user waved away. */
const CHECKS = 'kubri.releaseChecks';
const DISMISSED = 'kubri.releaseDismissed';

/** On unless turned off. */
export const releaseChecks = () => stored(CHECKS) !== 'off';
export const setReleaseChecks = (on: boolean) => store(CHECKS, on ? 'on' : 'off');
export const dismissedRelease = () => stored(DISMISSED);
export const dismissRelease = (version: string) => store(DISMISSED, version);

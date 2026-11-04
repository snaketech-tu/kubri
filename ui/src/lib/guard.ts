import type { Guard } from './api';

/**
 * Why Kubri does not change the cluster now, as the end of "Scale is not available: …". The
 * server's own reason wins when no switch in Kubri can lift it, so `--read-only` is never
 * blamed on k9s, and a switch the user can flip says where it is.
 */
export function readOnlyReason(guard: Guard, readOnlySetting: boolean, k9sReadOnly: boolean): string {
	if (guard.readOnly && guard.fixed && guard.reason) return `Kubri is read-only because ${guard.reason}`;
	if (readOnlySetting) return 'Read-only is on; switch it off in the header to make changes';
	if (guard.readOnly && guard.canUnlock) return 'this context is locked; unlock it with Read-only in the header';
	if (guard.readOnly && guard.reason) return `Kubri is read-only because ${guard.reason}`;
	if (k9sReadOnly) return 'Kubri is read-only because readOnly is set in the k9s configuration';
	return 'Kubri does not change this cluster';
}

/** The read-only switch in the header can do nothing: a snapshot, `--read-only` or k9s `readOnly`. */
export function readOnlyFixed(guard: Guard, k9sReadOnly: boolean, readOnlySetting: boolean): boolean {
	if (guard.fixed) return true;
	return k9sReadOnly || (guard.readOnly && !guard.canUnlock && !readOnlySetting);
}

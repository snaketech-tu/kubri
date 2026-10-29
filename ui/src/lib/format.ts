/** Formats an age the way kubectl does: 42s, 5m, 3h, 12d. */
export function age(timestamp: string | null | undefined, now: number): string {
	if (!timestamp) return '';
	const seconds = Math.max(0, Math.floor((now - Date.parse(timestamp)) / 1000));
	if (Number.isNaN(seconds)) return '';
	if (seconds < 60) return `${seconds}s`;
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 48) return `${hours}h`;
	const days = Math.floor(hours / 24);
	return days < 730 ? `${days}d` : `${Math.floor(days / 365)}y`;
}

/** A time before or after `now`: `3d ago`, `in 60d`. */
export function relative(timestamp: string | null | undefined, now: number): string {
	if (!timestamp || Number.isNaN(Date.parse(timestamp))) return '';
	const at = Date.parse(timestamp);
	return at > now ? `in ${age(new Date(now).toISOString(), at)}` : `${age(timestamp, now) || '0s'} ago`;
}

/** CPU in millicores, like `kubectl top`: 250m, 1.5. */
export function cpu(millicores: number): string {
	if (millicores >= 10_000) return `${(millicores / 1000).toFixed(1)}`;
	return `${Math.round(millicores)}m`;
}

/** Memory in bytes, in binary units: 512Mi, 1.5Gi. */
export function memory(bytes: number): string {
	const units = ['', 'Ki', 'Mi', 'Gi', 'Ti'];
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	const digits = unit >= 3 && value < 10 ? 1 : 0;
	return `${value.toFixed(digits)}${units[unit]}`;
}

export function errorMessage(err: unknown): string {
	return err instanceof Error ? err.message : String(err);
}

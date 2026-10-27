/** Formats an age the way kubectl does: 42s, 5m, 3h, 12d. */
export function age(timestamp: string | undefined, now: number): string {
	if (!timestamp) return '';
	const seconds = Math.max(0, Math.floor((now - Date.parse(timestamp)) / 1000));
	if (seconds < 60) return `${seconds}s`;
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 48) return `${hours}h`;
	const days = Math.floor(hours / 24);
	return days < 730 ? `${days}d` : `${Math.floor(days / 365)}y`;
}

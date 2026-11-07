import { t } from './i18n';
import { coreText } from './i18n/errors';
import { units } from './units.svelte';

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

/** Seconds as an age reads: 42s, 5m, 3h, 12d. */
function span(seconds: number): string {
	return age(new Date(0).toISOString(), seconds * 1000);
}

/**
 * kubectl's restarts, `5 (119s ago)`, with the time as ages read elsewhere in Kubri:
 * `5 · 1m ago`. Other text is left as it is.
 */
export function restarts(value: string): string {
	const m = /^(\d+) \(((?:\d+[dhms])+) ago\)$/.exec(value.trim());
	if (!m) return value;
	const seconds = [...m[2].matchAll(/(\d+)([dhms])/g)].reduce(
		(n, [, amount, unit]) => n + Number(amount) * { d: 86_400, h: 3600, m: 60, s: 1 }[unit as 'd' | 'h' | 'm' | 's'],
		0
	);
	return `${m[1]} · ${t('{age} ago', { age: span(seconds) })}`;
}

/** A time before or after `now`: `3d ago`, `in 60d`. */
export function relative(timestamp: string | null | undefined, now: number): string {
	if (!timestamp || Number.isNaN(Date.parse(timestamp))) return '';
	const at = Date.parse(timestamp);
	return at > now ? t('in {age}', { age: age(new Date(now).toISOString(), at) }) : t('{age} ago', { age: age(timestamp, now) || '0s' });
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

/** Cores with as many decimals as a small amount needs: 4, 1.5, 0.25, 0.05, and <0.01 for less. */
export function cores(millicores: number): string {
	const value = millicores / 1000;
	if (value === 0) return '0';
	if (value < 0.01) return '<0.01';
	const digits = value >= 10 ? 0 : value >= 1 ? 1 : 2;
	return String(Number(value.toFixed(digits)));
}

/** Memory for people, in binary units with a space: 384 KiB, 13 MiB, 1.5 GiB. */
export function bytes(amount: number): string {
	const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
	let value = amount;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	const digits = unit >= 2 && value < 10 ? 1 : 0;
	return `${Number(value.toFixed(digits))} ${units[unit]}`;
}

/**
 * CPU as the person chose to read it: cores (0.25), or millicores like kubectl (250m). With
 * `unit`, cores say so (0.25 cores), for text that has no column title to name the unit.
 */
export function cpuText(millicores: number, unit = false): string {
	if (units.kubectl) return cpu(millicores);
	if (!unit) return cores(millicores);
	const n = cores(millicores);
	return n === '1' ? t('1 core') : t('{n} cores', { n });
}

/** Memory as the person chose to read it: 512 MiB, or like kubectl, 512Mi. */
export function memoryText(amount: number): string {
	return units.kubectl ? memory(amount) : bytes(amount);
}

/** What went wrong, for people: an error from the core in the UI's language where Kubri knows it. */
export function errorMessage(err: unknown): string {
	return coreText(err instanceof Error ? err.message : String(err));
}

/**
 * The name a pod's workload gave it: `frontend-6f8dcb7dd4-c5zl6` came from `frontend`. A name
 * that does not look generated has no prefix.
 */
export function workloadPrefix(podName: string): string | null {
	const parts = podName.split('-');
	return parts.length >= 3 ? parts.slice(0, -2).join('-') : null;
}

/** Whether an API error says the object is not there (any more). */
export function isNotFound(message: string): boolean {
	return /"[^"]*" not found/i.test(message);
}

/**
 * A container image in pieces that may break onto the next line after each `/`, `:` or `@`, with
 * a digest cut to its first 12 characters. The full image belongs in a tooltip.
 */
export function imageParts(image: string): string[] {
	const short = image.replace(/(@sha256:[0-9a-f]{12})[0-9a-f]{20,}$/, '$1…');
	return short.split(/(?<=[/:@])/);
}

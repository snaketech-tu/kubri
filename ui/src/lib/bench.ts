import type { BenchInfo } from './api';

/** A response time in seconds, in the unit that reads best: 850µs, 12.3ms, 1.24s. */
export function duration(seconds: number | null | undefined): string {
	if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return '—';
	if (seconds < 0.001) return `${Math.round(seconds * 1e6)}µs`;
	if (seconds < 1) {
		const ms = seconds * 1000;
		return `${ms < 10 ? ms.toFixed(2) : ms < 100 ? ms.toFixed(1) : Math.round(ms)}ms`;
	}
	return `${seconds.toFixed(seconds < 10 ? 2 : 1)}s`;
}

/** A size in binary units with one decimal while small: 812 B, 1.8 KiB, 24 MiB. */
export function bytes(n: number): string {
	const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
	let value = n;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	return `${unit && value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** Requests per second, compact: 0.5, 38.2, 1,204. */
export function rate(perSecond: number): string {
	if (perSecond < 100) return perSecond.toFixed(1);
	return Math.round(perSecond).toLocaleString('en-US');
}

/** How far a run is, from 0 to 1, by requests or by time. */
export function progress(run: BenchInfo): number {
	if (run.state !== 'running') return 1;
	if (run.requests) return Math.min(1, run.completed / run.requests);
	if (run.durationSecs) return Math.min(1, run.elapsed / run.durationSecs);
	return 0;
}

/** 2xx fine, 3xx neutral, 4xx needs a look, 5xx broken. */
export function statusTone(code: number): 'ok' | 'muted' | 'warn' | 'bad' {
	if (code >= 500) return 'bad';
	if (code >= 400) return 'warn';
	if (code >= 300) return 'muted';
	return 'ok';
}

/** Parses `Name: value` lines into headers. Empty lines are skipped. */
export function parseHeaders(text: string): Record<string, string> {
	const headers: Record<string, string> = {};
	for (const line of text.split('\n')) {
		const trimmed = line.trim();
		if (!trimmed) continue;
		const i = trimmed.indexOf(':');
		if (i <= 0) throw new Error(`Expected "Name: value", got "${trimmed}"`);
		headers[trimmed.slice(0, i).trim()] = trimmed.slice(i + 1).trim();
	}
	return headers;
}

/** Round tick values from 0 up to at least `max`, about `count` of them. */
export function ticks(max: number, count = 4): number[] {
	if (max <= 0) return [0];
	const raw = max / count;
	const magnitude = 10 ** Math.floor(Math.log10(raw));
	const step = [1, 2, 5, 10].map((m) => m * magnitude).find((s) => s >= raw) ?? 10 * magnitude;
	const out: number[] = [];
	for (let v = 0; v < max + step; v += step) {
		out.push(v);
		if (v >= max) break;
	}
	return out;
}

/** The path and query of a benchmark URL, which is what tells runs apart. */
export function pathOf(url: string): string {
	try {
		const u = new URL(url);
		return u.pathname + u.search;
	} catch {
		return url;
	}
}

/** Requests that got a response, whatever its status; the rest failed with an error. */
export function responses(run: BenchInfo): number {
	return run.results?.statusCodes.reduce((n, s) => n + s.count, 0) ?? 0;
}

/** The share of failed requests: errors and 5xx responses. */
export function failureShare(run: BenchInfo): number {
	const r = run.results;
	if (!r || !r.total) return 0;
	const errors = r.errors.reduce((n, e) => n + e.count, 0);
	const server = r.statusCodes.filter((s) => s.code >= 500).reduce((n, s) => n + s.count, 0);
	return (errors + server) / r.total;
}

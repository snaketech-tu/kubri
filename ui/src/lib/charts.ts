import type { ChartRange, ChartSeries, ChartUnit, ResourceRef } from './api';
import { cpu, memory } from './format';

/** The time ranges the charts offer, as on most dashboards. */
export const RANGES: { label: string; seconds: number }[] = [
	{ label: '1h', seconds: 3600 },
	{ label: '6h', seconds: 6 * 3600 },
	{ label: '24h', seconds: 24 * 3600 },
	{ label: '7d', seconds: 7 * 24 * 3600 }
];

export const DEFAULT_RANGE: ChartRange = { seconds: 3600 };

/** A value in the chart's unit, short enough for an axis. */
export function formatValue(unit: ChartUnit, value: number): string {
	switch (unit) {
		case 'millicores':
			// Idle pods use a fraction of a millicore; 0m would hide that they run.
			return value > 0 && value < 9.95 ? `${round(value)}m` : cpu(value);
		case 'bytes':
			return memory(value);
		case 'bytesPerSecond':
			return `${memory(value)}B/s`;
		case 'perSecond':
			return `${round(value)}/s`;
		case 'count':
			return round(value);
	}
}

function round(value: number): string {
	if (value === 0) return '0';
	if (Math.abs(value) >= 100) return String(Math.round(value));
	if (Math.abs(value) >= 1) return String(Math.round(value * 10) / 10);
	return value.toPrecision(2).replace(/\.?0+$/, '');
}

/**
 * Round steps for a value axis from zero to at least `max`: about `count` of them, at 1, 2 or 5
 * times a power of ten, or of 1024 for bytes, so the labels read as 256Mi rather than 268M.
 */
export function valueTicks(max: number, unit: ChartUnit, count = 3): number[] {
	if (!(max > 0)) return [0, 1];
	const binary = unit === 'bytes' || unit === 'bytesPerSecond';
	const raw = max / count;
	let step: number;
	if (binary && raw >= 1024) {
		const power = 1024 ** Math.floor(Math.log(raw) / Math.log(1024));
		const scaled = raw / power;
		const nice = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024].find((n) => n >= scaled) ?? 1024;
		step = nice * power;
	} else {
		const power = 10 ** Math.floor(Math.log10(raw));
		const scaled = raw / power;
		step = (scaled <= 1 ? 1 : scaled <= 2 ? 2 : scaled <= 5 ? 5 : 10) * power;
	}
	// Counts are whole.
	if (unit === 'count') step = Math.max(1, Math.round(step));
	const ticks = [0];
	while (ticks[ticks.length - 1] < max) ticks.push(ticks[ticks.length - 1] + step);
	return ticks;
}

/** Times for the time axis, on round minutes, hours or days. */
export function timeTicks(start: number, end: number, count = 4): number[] {
	const span = end - start;
	const steps = [60, 300, 600, 900, 1800, 3600, 2 * 3600, 3 * 3600, 6 * 3600, 12 * 3600, 86400, 2 * 86400];
	const step = steps.find((s) => span / s <= count) ?? 7 * 86400;
	// Round in local time, so hours land on the hour where the reader is.
	const offset = new Date(start * 1000).getTimezoneOffset() * 60;
	const first = Math.ceil((start - offset) / step) * step + offset;
	const ticks = [];
	for (let t = first; t <= end; t += step) ticks.push(t);
	return ticks;
}

/** A time for the axis: the hour within a day, the day beyond. */
export function timeLabel(at: number, span: number): string {
	const date = new Date(at * 1000);
	if (span > 2 * 86400) return date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' });
	return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

/**
 * The color slot of each line: the same name keeps its color whatever else changes, and the
 * line that adds up the rest is gray.
 */
export function colorSlots(series: ChartSeries[]): Map<string, number | null> {
	const named = series.map((s) => s.name).filter((n) => !isOthers(n));
	const sorted = [...named].sort();
	return new Map(series.map((s) => [s.name, isOthers(s.name) ? null : (sorted.indexOf(s.name) % 8) + 1]));
}

export function isOthers(name: string) {
	return /^\d+ others$/.test(name);
}

/** The value of a line at the time nearest `at`, if it has one within half a step. */
export function valueAt(series: ChartSeries, at: number, step: number): number | null {
	let best: [number, number] | null = null;
	for (const point of series.points) {
		if (!best || Math.abs(point[0] - at) < Math.abs(best[0] - at)) best = point;
	}
	return best && Math.abs(best[0] - at) <= step / 2 ? best[1] : null;
}

/** Whether Kubri draws charts for objects of `resource`: pods, workloads, nodes, namespaces. */
export function hasCharts(resource: Pick<ResourceRef, 'group' | 'plural'>): boolean {
	const key = `${resource.group}/${resource.plural}`;
	return [
		'/pods',
		'/nodes',
		'/namespaces',
		'apps/deployments',
		'apps/statefulsets',
		'apps/daemonsets',
		'apps/replicasets',
		'batch/jobs',
		'batch/cronjobs'
	].includes(key);
}

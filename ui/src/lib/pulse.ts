import type { KindPulse, Utilization } from './api';

const KIND_LABELS: Record<string, string> = {
	HorizontalPodAutoscaler: 'Autoscalers',
	PersistentVolumeClaim: 'Volume claims',
	PersistentVolume: 'Volumes'
};

/** The plural name a pulse tile shows. */
export function kindLabel(pulse: KindPulse) {
	return KIND_LABELS[pulse.kind] ?? `${pulse.kind}s`;
}

/** How full a resource is, from 0 to 1: measured use, or requests without metrics-server. */
export function fill(u: Utilization): { value: number; measured: boolean } | null {
	if (!u.capacity) return null;
	if (u.used !== null) return { value: u.used / u.capacity, measured: true };
	if (u.requested !== null) return { value: u.requested / u.capacity, measured: false };
	return null;
}

/** Meter severity: the fill turns amber, then red, as the cluster fills up. */
export function meterTone(value: number): 'accent' | 'warn' | 'bad' {
	return value >= 0.9 ? 'bad' : value >= 0.75 ? 'warn' : 'accent';
}

export interface Sample {
	/** Milliseconds since the epoch. */
	at: number;
	value: number;
}

/** How far back the trend lines go. */
export const HISTORY_MS = 10 * 60 * 1000;

/** Adds a sample and drops the ones that fell out of the window. */
export function record(history: Sample[], value: number | null, at: number): Sample[] {
	const kept = history.filter((s) => s.at > at - HISTORY_MS);
	return value === null ? kept : [...kept, { at, value }];
}

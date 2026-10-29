import type { KindPulse, RecentEvent, Utilization } from './api';

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

/** One object that warnings point at, however many warnings there were. */
export interface AttentionItem {
	/** The latest warning about the object. */
	event: RecentEvent;
	/** Every reason given, latest first. */
	reasons: string[];
	/** How often the warnings came, together. */
	count: number;
	severity: 'critical' | 'warning';
}

/**
 * Reasons that mean something is down or cannot start. Others, such as FailedScheduling or
 * Unhealthy, may resolve on their own and stay warnings.
 */
const CRITICAL = /BackOff|CrashLoop|OOM|Evicted|ImagePull|ErrImage|Error|^Failed(Create|Mount|AttachVolume)?$/;

/** Warnings grouped by the object they are about, the ones that mean an outage first. */
export function attentionItems(events: RecentEvent[]): AttentionItem[] {
	const items = new Map<string, AttentionItem>();
	const latest = [...events].sort((a, b) => (b.lastSeen ?? '').localeCompare(a.lastSeen ?? ''));
	for (const event of latest) {
		const { resource, namespace, name } = event.object;
		const key = `${resource.group}/${resource.plural}/${namespace ?? ''}/${name}`;
		const severity = CRITICAL.test(event.reason) ? 'critical' : 'warning';
		const item = items.get(key);
		if (!item) {
			items.set(key, { event, reasons: [event.reason], count: event.count, severity });
			continue;
		}
		if (!item.reasons.includes(event.reason)) item.reasons.push(event.reason);
		item.count += event.count;
		if (severity === 'critical') item.severity = 'critical';
	}
	const rank = (item: AttentionItem) => (item.severity === 'critical' ? 0 : 1);
	return [...items.values()].sort((a, b) => rank(a) - rank(b));
}

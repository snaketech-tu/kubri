import type { KindPulse, ObjectRef, RecentEvent, Utilization } from './api';
import { t } from './i18n';

const KIND_LABELS: Record<string, string> = {
	HorizontalPodAutoscaler: t('Autoscalers'),
	PersistentVolumeClaim: t('Volume claims'),
	PersistentVolume: t('Volumes')
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

/** One workload, or one object without a workload, that warnings point at, however many there were. */
export interface AttentionItem {
	/** The latest warning about it. */
	event: RecentEvent;
	/** What the item is about: the workload when the warnings came from its pods. */
	kind: string;
	name: string;
	namespace: string | null;
	/** The objects the warnings are about, such as the workload's pods, latest first. */
	objects: ObjectRef[];
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

const refKey = ({ resource, namespace, name }: ObjectRef) => `${resource.group}/${resource.plural}/${namespace ?? ''}/${name}`;

/**
 * Warnings grouped by the workload their pods belong to, or by the object they are about,
 * the ones that mean an outage first.
 */
export function attentionItems(events: RecentEvent[]): AttentionItem[] {
	const items = new Map<string, AttentionItem>();
	const latest = [...events].sort((a, b) => (b.lastSeen ?? '').localeCompare(a.lastSeen ?? ''));
	for (const event of latest) {
		const about = event.workload ?? { kind: event.objectKind, object: event.object };
		const key = refKey(about.object);
		const severity = CRITICAL.test(event.reason) ? 'critical' : 'warning';
		const item = items.get(key);
		if (!item) {
			items.set(key, {
				event,
				kind: about.kind,
				name: about.object.name,
				namespace: about.object.namespace ?? null,
				objects: [event.object],
				reasons: [event.reason],
				count: event.count,
				severity
			});
			continue;
		}
		if (!item.objects.some((o) => refKey(o) === refKey(event.object))) item.objects.push(event.object);
		if (!item.reasons.includes(event.reason)) item.reasons.push(event.reason);
		item.count += event.count;
		if (severity === 'critical') item.severity = 'critical';
	}
	const rank = (item: AttentionItem) => (item.severity === 'critical' ? 0 : 1);
	return [...items.values()].sort((a, b) => rank(a) - rank(b));
}

/** The container a "Back-off restarting failed container" warning names. */
export function restartingContainer(message: string): string | null {
	return /^Back-off restarting failed container\s+"?([^\s"]+)"?/i.exec(message)?.[1] ?? null;
}

/**
 * A kubelet text in plain words. The original stays in the tooltip. Unknown texts are cut at
 * the first sentence and at 140 characters. How often it came is shown beside it: that counts
 * the warnings, not restarts or pulls, and since the event began rather than in the last hour.
 */
export function shortMessage(reason: string, message: string): string {
	const back = restartingContainer(message);
	if (back) return t('Container {name} keeps restarting', { name: back });
	const pull = /(?:Failed to pull image|Error: ErrImagePull|ImagePullBackOff).*?"([^"]+)"/i.exec(message);
	if (pull) return t('Cannot pull image {image}', { image: pull[1] });
	if (/^Back-off pulling image\s+"([^"]+)"/i.test(message)) {
		return t('Cannot pull image {image}', { image: /"([^"]+)"/.exec(message)?.[1] ?? '' });
	}
	if (/OOMKilled/i.test(message) || reason === 'OOMKilling') return t('Ran out of memory and was stopped');
	const nodes = /^(0\/\d+ nodes are available)/.exec(message);
	if (nodes) return t('Cannot be scheduled: {nodes}', { nodes: nodes[1] });
	const first = message.split(/(?<=\.)\s/)[0].replace(/\.$/, '');
	return first.length > 140 ? `${first.slice(0, 137)}…` : first;
}

/** Kinds with problems first (keeping their order), then the healthy ones. */
export function splitKinds(kinds: KindPulse[]): { problems: KindPulse[]; healthy: KindPulse[] } {
	const sick = (k: KindPulse) => Boolean(k.error) || k.failed > 0 || k.warning > 0;
	return { problems: kinds.filter(sick), healthy: kinds.filter((k) => !sick(k)) };
}

/** How many polls a list row's trend keeps. */
export const TRAIL_LENGTH = 20;

/** Adds a reading to a row's trend and keeps the latest ones. */
export function pushTrail(trail: number[] | undefined, value: number): number[] {
	return [...(trail ?? []), value].slice(-TRAIL_LENGTH);
}

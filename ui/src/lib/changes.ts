import type { ChangeKind, HistoryEntry, Span } from './api';

/** What the What changed page shows: a namespace (all when empty), one object, the last hours. */
export interface ChangesFilter {
	namespace?: string;
	kind?: string;
	name?: string;
	hours?: number;
}

/** Which changes a filter chip stands for. */
export const CHANGE_GROUPS: { label: string; kinds: ChangeKind[] }[] = [
	{ label: 'Rollouts', kinds: ['image', 'rollout', 'restarted'] },
	{ label: 'Scaling', kinds: ['scaled'] },
	{ label: 'Config', kinds: ['data'] },
	{ label: 'Spec', kinds: ['spec', 'metadata'] },
	{ label: 'Created and deleted', kinds: ['created', 'deleted'] },
	{ label: 'Nodes', kinds: ['node'] },
	{ label: 'Warnings', kinds: ['warning'] }
];

/** How far back to look, by its label in the range picker; `0` for everything kept. */
export const RANGES: { label: string; hours: number }[] = [
	{ label: 'Last hour', hours: 1 },
	{ label: 'Last 6 hours', hours: 6 },
	{ label: 'Last 24 hours', hours: 24 },
	{ label: 'Last 7 days', hours: 24 * 7 },
	{ label: 'Everything kept', hours: 0 }
];

const TAGS: Record<ChangeKind, string> = {
	created: 'Created',
	deleted: 'Deleted',
	scaled: 'Scaled',
	image: 'Image',
	restarted: 'Restart',
	rollout: 'Rollout',
	data: 'Data',
	spec: 'Spec',
	metadata: 'Labels',
	node: 'Node',
	warning: 'Warning'
};

export function changeTag(kind: ChangeKind): string {
	return TAGS[kind] ?? kind;
}

/** The tone of a change's tag: warnings and deletions stand out, rollouts are accented. */
export function changeTone(kind: ChangeKind): 'bad' | 'warn' | 'accent' | 'plain' {
	if (kind === 'warning') return 'warn';
	if (kind === 'deleted') return 'bad';
	if (kind === 'image' || kind === 'rollout' || kind === 'restarted' || kind === 'scaled' || kind === 'data') return 'accent';
	return 'plain';
}

export type TimelineItem =
	| { type: 'day'; key: string; label: string }
	| { type: 'entry'; key: string; entry: HistoryEntry }
	/** Kubri was not watching from `from` to `to`; `to` is null for "until now" and `from` for "since ever". */
	| { type: 'gap'; key: string; from: number | null; to: number | null };

function dayLabel(at: number, now: number): string {
	const day = new Date(at);
	const today = new Date(now);
	const start = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	const days = Math.round((start(today) - start(day)) / 86_400_000);
	if (days === 0) return 'Today';
	if (days === 1) return 'Yesterday';
	return day.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: day.getFullYear() === today.getFullYear() ? undefined : 'numeric' });
}

/** Gaps shorter than this are not worth a line. */
const MIN_GAP_MS = 2 * 60_000;

/**
 * The timeline as shown: entries newest first under day headings, with a line where Kubri was not
 * watching. `watched` is newest first, as the backend sends it; `since` is where the range starts,
 * or null for everything kept; `complete` says that no older entries are left to load, so the time
 * before the first watched span is a gap too.
 */
export function timeline(
	entries: HistoryEntry[],
	watched: Span[],
	since: number | null,
	now: number,
	recording: boolean,
	complete: boolean
): TimelineItem[] {
	const gaps: { from: number | null; to: number | null }[] = [];
	const newest = watched[0];
	// Not watching now: the gap runs from the end of the last span until now.
	if (!recording && (!newest || now - newest.to > MIN_GAP_MS)) gaps.push({ from: newest?.to ?? null, to: null });
	for (let i = 0; i + 1 < watched.length; i++) {
		const newer = watched[i];
		const older = watched[i + 1];
		if (newer.from - older.to > MIN_GAP_MS) gaps.push({ from: older.to, to: newer.from });
	}
	const oldest = watched[watched.length - 1];
	if (complete && oldest && (since === null || oldest.from - since > MIN_GAP_MS)) gaps.push({ from: null, to: oldest.from });

	// Merge entries and gaps by time, newest first; a gap sits at its newer end.
	const at = (gap: { to: number | null }) => gap.to ?? now;
	const items: TimelineItem[] = [];
	let day = '';
	let g = 0;
	const addDay = (time: number) => {
		const label = dayLabel(time, now);
		if (label !== day) {
			day = label;
			items.push({ type: 'day', key: `day-${time}`, label });
		}
	};
	const addGap = () => {
		const gap = gaps[g++];
		addDay(at(gap));
		items.push({ type: 'gap', key: `gap-${gap.from}-${gap.to}`, ...gap });
	};
	for (const entry of entries) {
		while (g < gaps.length && at(gaps[g]) >= entry.at) addGap();
		addDay(entry.at);
		items.push({ type: 'entry', key: `entry-${entry.id}`, entry });
	}
	// Gaps older than the oldest entry only once nothing older is left to load.
	while (g < gaps.length && (complete || (entries.length > 0 && at(gaps[g]) >= entries[entries.length - 1].at))) addGap();
	return items;
}

/** `:changes shop 6h`: an optional namespace and an optional range like `30m`, `6h` or `7d`. */
export function parseChangesArgs(args: string[]): { namespace?: string; hours?: number } {
	const out: { namespace?: string; hours?: number } = {};
	for (const arg of args) {
		const range = /^(\d+(?:\.\d+)?)(m|h|d)$/.exec(arg);
		if (range) {
			const n = Number(range[1]);
			out.hours = range[2] === 'm' ? n / 60 : range[2] === 'd' ? n * 24 : n;
		} else if (arg) {
			out.namespace = arg;
		}
	}
	return out;
}

/** "14:03:21", or with the date for older times. */
export function clock(at: number): string {
	return new Date(at).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

/** A time for the gap line: the clock today, the date and clock otherwise. */
export function when(at: number, now: number): string {
	const d = new Date(at);
	const sameDay = new Date(now).toDateString() === d.toDateString();
	return sameDay ? clock(at) : d.toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
}

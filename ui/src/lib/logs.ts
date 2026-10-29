import type { LogLevel, LogSource } from './api';
import { logMatcher } from './pods';

/** A line in the log view: a log line, a stream error, a marker from Kubri or a mark drawn with `m`. */
export interface ViewLine {
	id: number;
	source: number | null;
	text: string;
	time?: string;
	level?: LogLevel;
	fields?: Record<string, unknown>;
	continued?: boolean;
	kind: 'line' | 'error' | 'marker' | 'mark';
}

/** What the view shows for one or more lines: repeats are counted, continuation lines fold under their first line. */
export interface Row {
	line: ViewLine;
	/** How often the same line came in a row. */
	count: number;
	/** Lines that continue this one, such as stack frames. */
	more: ViewLine[];
}

export const LEVELS: LogLevel[] = ['debug', 'info', 'warn', 'error'];

/** Folds repeated lines and continuation lines into rows; without `fold`, one row per line. */
export function foldRows(lines: ViewLine[], fold: boolean): Row[] {
	const rows: Row[] = [];
	// The last row of each source, which a continuation line belongs to.
	const last = new Map<number, Row>();
	for (const line of lines) {
		if (!fold || line.kind !== 'line' || line.source === null) {
			rows.push({ line, count: 1, more: [] });
			if (line.kind !== 'line') last.clear();
			continue;
		}
		const previous = last.get(line.source);
		if (line.continued && previous) {
			previous.more.push(line);
			continue;
		}
		const tail = rows[rows.length - 1];
		if (tail && tail === previous && !tail.more.length && tail.line.text === line.text && tail.line.kind === 'line') {
			tail.count++;
			continue;
		}
		const row = { line, count: 1, more: [] };
		rows.push(row);
		last.set(line.source, row);
	}
	return rows;
}

type Op = '=' | '!=' | '>=' | '<=' | '>' | '<' | '~';

export interface FieldTerm {
	key: string;
	op: Op;
	value: string;
}

/** A search: plain text or /regex/, or field terms such as `level=error status>=500 path~/api`. */
export interface Query {
	text: string;
	fields: FieldTerm[];
}

const TERM = /^([A-Za-z_@][\w.@-]*)(!=|>=|<=|=|>|<|~)(.*)$/;

/** Field terms when every word of `search` is one; text otherwise. */
export function parseQuery(search: string): Query {
	const words = search.trim().split(/\s+/).filter(Boolean);
	const terms = words.map((w) => TERM.exec(w));
	if (words.length && terms.every((t) => t && t[3] !== '')) {
		return {
			text: '',
			fields: terms.map((t) => ({ key: t![1], op: t![2] as Op, value: unquote(t![3]) }))
		};
	}
	return { text: search.trim(), fields: [] };
}

function unquote(value: string) {
	return /^"(.*)"$/.exec(value)?.[1] ?? value;
}

/** A field's value, by its name in any case or by a dotted path into nested objects. */
export function fieldValue(fields: Record<string, unknown> | undefined, key: string): unknown {
	if (!fields) return undefined;
	const lower = key.toLowerCase();
	for (const [k, v] of Object.entries(fields)) if (k.toLowerCase() === lower) return v;
	let value: unknown = fields;
	for (const part of key.split('.')) {
		if (!value || typeof value !== 'object') return undefined;
		const entry = Object.entries(value as Record<string, unknown>).find(
			([k]) => k.toLowerCase() === part.toLowerCase()
		);
		if (!entry) return undefined;
		value = entry[1];
	}
	return value;
}

function matchTerm(term: FieldTerm, actual: unknown): boolean {
	if (actual === undefined || actual === null) return term.op === '!=';
	const text = typeof actual === 'object' ? JSON.stringify(actual) : String(actual);
	const want = term.value;
	const a = Number(text);
	const b = Number(want);
	const numeric = text.trim() !== '' && want.trim() !== '' && !isNaN(a) && !isNaN(b);
	switch (term.op) {
		case '=':
			return numeric ? a === b : text.toLowerCase() === want.toLowerCase();
		case '!=':
			return numeric ? a !== b : text.toLowerCase() !== want.toLowerCase();
		case '~':
			return logMatcher(want)(text);
		default: {
			if (!numeric) return false;
			if (term.op === '>') return a > b;
			if (term.op === '<') return a < b;
			if (term.op === '>=') return a >= b;
			return a <= b;
		}
	}
}

/** Whether `line` matches `query`. `level`, `pod` and `container` work on every line, other keys on its fields. */
export function lineMatcher(query: Query, sources: LogSource[]): (line: ViewLine) => boolean {
	if (query.fields.length) {
		return (line) =>
			query.fields.every((term) => {
				const key = term.key.toLowerCase();
				const source = line.source === null ? undefined : sources[line.source];
				const own = fieldValue(line.fields, term.key);
				if (own !== undefined) return matchTerm(term, own);
				if (key === 'level') return levelTerm(term, line.level);
				if (key === 'pod') return matchTerm(term, source?.pod);
				if (key === 'container') return matchTerm(term, source?.container);
				return matchTerm(term, undefined);
			});
	}
	const text = logMatcher(query.text);
	return (line) => text(line.text);
}

/** `level>=warn` compares levels in order; `level=warn` names one. */
function levelTerm(term: FieldTerm, level: LogLevel | undefined): boolean {
	const want = LEVELS.indexOf(term.value.toLowerCase().replace(/^warning$/, 'warn') as LogLevel);
	if (want < 0 || !['>=', '<=', '>', '<'].includes(term.op)) return matchTerm(term, level);
	if (!level) return false;
	const have = LEVELS.indexOf(level);
	if (term.op === '>') return have > want;
	if (term.op === '<') return have < want;
	if (term.op === '>=') return have >= want;
	return have <= want;
}

/** Whether a row stays at a minimum level; lines without a level are kept only when every level is shown. */
export function atLevel(line: ViewLine, min: LogLevel | ''): boolean {
	if (!min || line.kind !== 'line') return true;
	return !!line.level && LEVELS.indexOf(line.level) >= LEVELS.indexOf(min);
}

const LEVEL_KEYS = new Set(['level', 'lvl', 'severity', 'loglevel', 'log.level', 'levelname', '@l', 'log_level']);
const TIME_KEYS = new Set(['time', 'ts', 'timestamp', '@t', '@timestamp', 'date', 'datetime']);
const MESSAGE_KEYS = ['msg', 'message', '@m', '@mt', 'text', 'event'];

/** The field names seen in `lines`, the most frequent first. */
export function fieldNames(lines: ViewLine[]): string[] {
	const counts = new Map<string, number>();
	for (const line of lines)
		for (const key of Object.keys(line.fields ?? {})) counts.set(key, (counts.get(key) ?? 0) + 1);
	return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([key]) => key);
}

/** Columns to start a table with: the message and the next three most frequent fields, leaving out time and level, which have their own columns. */
export function defaultColumns(names: string[]): string[] {
	const rest = names.filter((n) => !LEVEL_KEYS.has(n.toLowerCase()) && !TIME_KEYS.has(n.toLowerCase()));
	const message = rest.find((n) => MESSAGE_KEYS.includes(n.toLowerCase()));
	return [...(message ? [message] : []), ...rest.filter((n) => n !== message).slice(0, 3)];
}

/** A field value as a table cell shows it. */
export function cell(value: unknown): string {
	if (value === undefined || value === null) return '';
	return typeof value === 'object' ? JSON.stringify(value) : String(value);
}

/** The line's text, pretty-printed when it is JSON. */
export function pretty(line: ViewLine): string {
	if (!line.fields || !line.text.trim().startsWith('{')) return line.text;
	return JSON.stringify(line.fields, null, 2);
}

/** A line's time as the view shows it: local time with milliseconds. */
export function shortTime(time: string | undefined): string {
	if (!time) return '';
	const date = new Date(time);
	if (isNaN(date.getTime())) return time;
	const pad = (n: number, w = 2) => String(n).padStart(w, '0');
	return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}

/** The text of rows as shown, for copying and saving: repeats as `× n`, folded lines as a count. */
export function rowText(
	rows: Row[],
	options: { label: (line: ViewLine) => string; time: boolean; open: Set<number> }
): string[] {
	const out: string[] = [];
	const text = (line: ViewLine) => (options.time && line.time ? `${line.time} ` : '') + options.label(line) + line.text;
	for (const row of rows) {
		const { line } = row;
		if (line.kind === 'mark') {
			out.push('-----');
			continue;
		}
		if (line.kind === 'marker') {
			out.push(`----- ${line.text} -----`);
			continue;
		}
		out.push(text(line) + (row.count > 1 ? `  (× ${row.count})` : ''));
		if (!row.more.length) continue;
		if (options.open.has(line.id)) out.push(...row.more.map(text));
		else out.push(`    … ${row.more.length} more ${row.more.length === 1 ? 'line' : 'lines'}`);
	}
	return out;
}

export interface Bucket {
	from: number;
	to: number;
	lines: number;
	errors: number;
	warnings: number;
	/** Kubri's markers in this bucket, such as a pod that joined or a log that ended. */
	markers: string[];
	/** The first row in the bucket, to jump to. */
	first: number;
}

/**
 * Lines per time slot, for the rate strip above the log. `null` while fewer than two lines carry a time.
 * `changes` are recorded changes, like rollouts, marked where they fall within the lines' time.
 */
export function rate(rows: Row[], slots = 60, changes: { at: number; text: string }[] = []): Bucket[] | null {
	const timed: { at: number; index: number; row: Row }[] = [];
	rows.forEach((row, index) => {
		const at = row.line.time ? Date.parse(row.line.time) : NaN;
		if (!isNaN(at)) timed.push({ at, index, row });
	});
	if (timed.length < 2) return null;
	// Lines of several pods need not come in time order.
	let start = Infinity;
	let end = -Infinity;
	for (const { at } of timed) {
		start = Math.min(start, at);
		end = Math.max(end, at);
	}
	if (end <= start) return null;
	const step = (end - start) / slots;
	const buckets: Bucket[] = Array.from({ length: slots }, (_, i) => ({
		from: start + i * step,
		to: start + (i + 1) * step,
		lines: 0,
		errors: 0,
		warnings: 0,
		markers: [],
		first: -1
	}));
	for (const { at, index, row } of timed) {
		const b = buckets[Math.min(slots - 1, Math.max(0, Math.floor((at - start) / step)))];
		if (b.first < 0 || index < b.first) b.first = index;
		if (row.line.kind === 'marker') {
			b.markers.push(row.line.text);
			continue;
		}
		const n = row.count + row.more.length;
		b.lines += n;
		if (row.line.level === 'error') b.errors += n;
		else if (row.line.level === 'warn') b.warnings += n;
	}
	for (const change of changes) {
		if (change.at < start || change.at > end) continue;
		buckets[Math.min(slots - 1, Math.floor((change.at - start) / step))].markers.push(change.text);
	}
	return buckets;
}

import type { ContextInfo, FieldTest, ResourceInfo, SearchRequest } from './api';
import { matchResources } from './commands';

/** A query as typed, as the core takes it, and what in it was not understood. */
export interface ParsedSearch {
	request: SearchRequest;
	/** Words that named no kind or context, and the like. */
	problems: string[];
}

const OPS = ['!=', '>=', '<=', '!~', '=', '~', '>', '<'] as const;

/** Splits on spaces, keeping quoted parts together: `"a b"` and `'a b'`. */
export function words(query: string): string[] {
	const out: string[] = [];
	const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
	for (const m of query.matchAll(re)) out.push(m[1] ?? m[2] ?? m[3]);
	return out;
}

/** `status.phase!=Running`: a path with a dot, an operator and a value. */
export function fieldTest(word: string): FieldTest | null {
	const m = /^([A-Za-z$.][\w.$[\]*'"\\/-]*?)(!=|>=|<=|!~|=|~|>|<)(.*)$/.exec(word);
	if (!m || !m[1].includes('.')) return null;
	const op = OPS.find((o) => o === m[2]);
	return op ? { path: m[1], op, value: m[3] } : null;
}

/**
 * Reads a search like `web kind:deploy,svc ns:shop -l tier=front status.phase!=Running ctx:*`.
 *
 * - words are part of the name, `/…/` a regular expression
 * - `kind:` (or `in:`) names kinds as the palette does; `kind:all` searches every kind
 * - `ns:` (or `-n`) one namespace; `-l` (or `label:`) a label selector
 * - `path op value` compares a field: `=`, `!=`, `~`, `!~`, `>`, `<`, `>=`, `<=`
 * - `ctx:*` searches every context, `ctx:a,b` some
 */
export function parseSearch(query: string, resources: ResourceInfo[], contexts: ContextInfo[]): ParsedSearch {
	const request: SearchRequest = { text: '', kinds: [], allKinds: false, namespace: null, labels: '', fields: [], contexts: [] };
	const problems: string[] = [];
	const text: string[] = [];
	const labels: string[] = [];
	const list = words(query);
	for (let i = 0; i < list.length; i++) {
		const word = list[i];
		const next = () => list[++i] ?? '';
		const prefix = /^(kind|in|ns|namespace|ctx|context|label|labels):(.*)$/i.exec(word);
		if (prefix) {
			const [, key, value] = prefix;
			switch (key.toLowerCase()) {
				case 'kind':
				case 'in':
					for (const name of value.split(',').filter(Boolean)) {
						if (name.toLowerCase() === 'all') {
							request.allKinds = true;
							continue;
						}
						const found = matchResources(name, resources)[0];
						if (found) {
							if (!request.kinds.some((k) => k.group === found.group && k.plural === found.plural)) {
								request.kinds.push({ group: found.group, version: found.version, plural: found.plural });
							}
						} else problems.push(`No kind is called ${name}`);
					}
					break;
				case 'ns':
				case 'namespace':
					request.namespace = value || null;
					break;
				case 'ctx':
				case 'context':
					if (value === '*' || value.toLowerCase() === 'all') request.contexts = contexts.map((c) => c.name);
					else
						for (const name of value.split(',').filter(Boolean)) {
							if (contexts.some((c) => c.name === name)) request.contexts.push(name);
							else problems.push(`No context is called ${name}`);
						}
					break;
				default:
					if (value) labels.push(value);
			}
			continue;
		}
		if (word === '-l') {
			const value = next();
			if (value) labels.push(value);
			continue;
		}
		if (word.startsWith('-l') && word.length > 2) {
			labels.push(word.slice(2).replace(/^=/, ''));
			continue;
		}
		if (word === '-n') {
			request.namespace = next() || null;
			continue;
		}
		if (word === '-A' || word === '--all-contexts') {
			if (word === '--all-contexts') request.contexts = contexts.map((c) => c.name);
			continue;
		}
		const field = fieldTest(word);
		if (field) {
			request.fields.push(field);
			continue;
		}
		text.push(word);
	}
	request.text = text.join(' ');
	request.labels = labels.join(',');
	return { request, problems };
}

/** Whether a search would list everything: nothing narrows it. */
export function unbounded(request: SearchRequest): boolean {
	return !request.text && !request.labels && !request.fields.length && !request.namespace && !request.kinds.length;
}

const RECENT_KEY = 'kubri.recent';
const RECENT_MAX = 12;

export interface RecentObject {
	context: string;
	kind: string;
	group: string;
	version: string;
	plural: string;
	namespace: string | null;
	name: string;
}

/** Objects opened lately, newest first, kept in the browser. */
export function recentObjects(): RecentObject[] {
	try {
		const parsed = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
}

/** Puts `object` first among the recent ones. */
export function remember(object: RecentObject, list = recentObjects()): RecentObject[] {
	const same = (o: RecentObject) =>
		o.context === object.context && o.group === object.group && o.plural === object.plural && o.namespace === object.namespace && o.name === object.name;
	const next = [object, ...list.filter((o) => !same(o))].slice(0, RECENT_MAX);
	try {
		localStorage.setItem(RECENT_KEY, JSON.stringify(next));
	} catch {
		// Private windows may refuse; the list then lasts until Kubri closes.
	}
	return next;
}

import type { ContextSettings } from './api';

/*
 * Which namespaces get production's care in a context that is not production as a whole. The core
 * decides the same for AI agents (`ContextSettings::production_by_name` and `production_by_labels`
 * in crates/kubri-core/src/settings.rs); keep the two alike.
 */

export type Labels = Record<string, string>;

/** Why a namespace counts as production: marked by its name, a name pattern, or a label rule. */
export interface ProductionRule {
	by: 'name' | 'pattern' | 'labels';
	/** The entry of the settings, like `shop`, `*-prod` or `env=production`. */
	rule: string;
}

/** Whether `name` matches `pattern`, where `*` stands for any text, also none. */
export function nameMatches(pattern: string, name: string): boolean {
	const parts = pattern.split('*');
	const first = parts.shift() ?? '';
	if (!name.startsWith(first)) return false;
	let rest = name.slice(first.length);
	const last = parts.pop();
	if (last === undefined) return rest === '';
	for (const part of parts) {
		const at = rest.indexOf(part);
		if (at === -1) return false;
		rest = rest.slice(at + part.length);
	}
	return rest.length >= last.length && rest.endsWith(last);
}

type Requirement = { op: '=' | '!=' | 'exists' | 'missing'; key: string; value: string };

const KEY = /^[A-Za-z0-9\-_./]+$/;
const VALUE = /^[A-Za-z0-9\-_.]*$/;

/** The parts of a label rule such as `env=production,!canary`; null when Kubri cannot read one. */
export function parseLabelRule(rule: string): Requirement[] | null {
	const out: Requirement[] = [];
	for (const part of rule.split(',').map((p) => p.trim()).filter(Boolean)) {
		let req: Requirement;
		const differs = part.indexOf('!=');
		const equals = part.indexOf('=');
		if (differs !== -1) req = { op: '!=', key: part.slice(0, differs).trim(), value: part.slice(differs + 2).trim() };
		else if (equals !== -1) req = { op: '=', key: part.slice(0, equals).trim(), value: part.slice(equals + 1).replace(/^=/, '').trim() };
		else if (part.startsWith('!')) req = { op: 'missing', key: part.slice(1).trim(), value: '' };
		else req = { op: 'exists', key: part, value: '' };
		if (!KEY.test(req.key) || !VALUE.test(req.value)) return null;
		out.push(req);
	}
	return out;
}

/** A label rule as the core keeps it: `key=value` parts joined by commas, without spaces. */
export function tidyLabelRule(rule: string): string | null {
	const parts = parseLabelRule(rule);
	if (!parts) return null;
	return parts
		.map((p) => (p.op === 'exists' ? p.key : p.op === 'missing' ? `!${p.key}` : `${p.key}${p.op}${p.value}`))
		.join(',');
}

/** Whether `labels` meet every part of `rule`; never for a rule Kubri cannot read or an empty one. */
export function labelsMatch(rule: string, labels: Labels): boolean {
	const parts = parseLabelRule(rule);
	if (!parts?.length) return false;
	return parts.every((p) => {
		const has = Object.hasOwn(labels, p.key);
		switch (p.op) {
			case '=':
				return has && labels[p.key] === p.value;
			case '!=':
				return !has || labels[p.key] !== p.value;
			case 'exists':
				return has;
			case 'missing':
				return !has;
		}
	});
}

type Rules = Pick<ContextSettings, 'productionNamespaces'> & Partial<Pick<ContextSettings, 'productionLabels'>>;

/**
 * What marks `namespace` as production in these settings: its own name first, then a pattern,
 * then a label rule. Labels left out are not known, and no label rule matches them here; the core
 * reads them itself before it lets an AI agent change anything.
 */
export function productionRule(settings: Rules, namespace: string, labels?: Labels): ProductionRule | null {
	const names = settings.productionNamespaces ?? [];
	if (names.includes(namespace)) return { by: 'name', rule: namespace };
	const pattern = names.find((p) => p.includes('*') && nameMatches(p, namespace));
	if (pattern) return { by: 'pattern', rule: pattern };
	if (labels) {
		const rule = (settings.productionLabels ?? []).find((r) => labelsMatch(r, labels));
		if (rule) return { by: 'labels', rule };
	}
	return null;
}

/** The names marked one by one, without the patterns. */
export function markedNames(settings: Rules): string[] {
	return (settings.productionNamespaces ?? []).filter((n) => !n.includes('*'));
}

/** The name patterns, like `*-prod`. */
export function namePatterns(settings: Rules): string[] {
	return (settings.productionNamespaces ?? []).filter((n) => n.includes('*'));
}

/** `names` marked as production one by one, or no longer, keeping the patterns; sorted and unique. */
export function withMarked(settings: Rules, names: string[], production: boolean): string[] {
	const out = new Set(settings.productionNamespaces ?? []);
	for (const name of names) {
		if (production) out.add(name);
		else out.delete(name);
	}
	return [...out].sort();
}

/** A word in a namespace's name that says production, like `prod` in `shop-prod`. */
const PRODUCTION_WORD = /(^|[-_.])(prod|production|prd|live)([-_.]|$)/;
/** Label keys teams use for the environment. */
const ENVIRONMENT_KEYS = ['env', 'environment', 'stage', 'tier', 'app.kubernetes.io/environment'];
const PRODUCTION_VALUE = /^(prod|production|prd|live)$/i;
/** Namespaces Kubernetes and its add-ons make; they say nothing about the user's environments. */
const SYSTEM = /^(kube-|openshift)/;

/** A namespace that looks like production, and the rule that would mark it and others like it. */
export interface Suggestion {
	name: string;
	/** A pattern like `*-prod`, or a label rule like `env=production`. */
	rule: string;
	by: 'pattern' | 'labels';
}

/**
 * Namespaces that look like production by a word in their name or an environment label, and are
 * not marked yet. Kubri only suggests them: marking is always the user's choice.
 */
export function suggestions(settings: Rules, namespaces: { name: string; labels?: Labels }[]): Suggestion[] {
	const out: Suggestion[] = [];
	for (const { name, labels = {} } of namespaces) {
		if (SYSTEM.test(name) || productionRule(settings, name, labels)) continue;
		const key = ENVIRONMENT_KEYS.find((k) => PRODUCTION_VALUE.test(labels[k] ?? ''));
		if (key) {
			out.push({ name, rule: `${key}=${labels[key]}`, by: 'labels' });
			continue;
		}
		const word = name.match(PRODUCTION_WORD);
		if (word) out.push({ name, rule: patternFor(name, word), by: 'pattern' });
	}
	return out.sort((a, b) => a.name.localeCompare(b.name));
}

/** The pattern a name with a production word suggests: `shop-prod` gives `*-prod`, `prod-eu` gives `prod-*`. */
function patternFor(name: string, word: RegExpMatchArray): string {
	const at = word.index ?? 0;
	const start = at + word[1].length;
	const end = start + word[2].length;
	const before = start > 0 ? `*${name[start - 1]}` : '';
	const after = end < name.length ? `${name[end]}*` : '';
	return `${before}${word[2]}${after}`;
}

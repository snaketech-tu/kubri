/** Merging an edit with what changed on the server meanwhile, as a three-way merge of JSON. */

type Json = unknown;

const IGNORED = new Set(['status', 'metadata.managedFields', 'metadata.generation', 'metadata.resourceVersion']);

function isObject(value: Json): value is Record<string, Json> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function deepEqual(a: Json, b: Json): boolean {
	if (a === b) return true;
	if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => deepEqual(v, b[i]));
	if (isObject(a) && isObject(b)) {
		const keys = Object.keys(a);
		return keys.length === Object.keys(b).length && keys.every((k) => k in b && deepEqual(a[k], b[k]));
	}
	return false;
}

/** An object without what the API server changes by itself: status, resourceVersion and the like. */
export function comparable(object: Json): Json {
	if (!isObject(object)) return object;
	const { status: _status, ...rest } = object;
	if (isObject(rest.metadata)) {
		const { managedFields: _m, generation: _g, resourceVersion: _r, ...metadata } = rest.metadata;
		rest.metadata = metadata;
	}
	return rest;
}

/** Whether `theirs` differs from `base` in more than what the API server changes by itself. */
export function changedMeaningfully(base: Json, theirs: Json): boolean {
	return !deepEqual(comparable(base), comparable(theirs));
}

/** Items of a list keyed by `name`, as containers, ports and volumes are; null when they are not. */
function byName(list: Json[]): Map<string, Json> | null {
	const map = new Map<string, Json>();
	for (const item of list) {
		if (!isObject(item) || typeof item.name !== 'string' || map.has(item.name)) return null;
		map.set(item.name, item);
	}
	return map;
}

function label(path: string, key: string | number) {
	return typeof key === 'number' ? `${path}[${key}]` : path ? `${path}.${key}` : key;
}

export interface Merged {
	merged: Json;
	/** Fields both sides changed differently, where `mine` was kept, as `spec.replicas`. */
	conflicts: string[];
}

/**
 * Applies the changes from `base` to `mine` onto `theirs`. Where both sides changed a field
 * differently, `mine` wins and the field is listed as a conflict. Lists of named items, such as
 * containers, merge item by item; other lists merge as a whole.
 */
export function threeWay(base: Json, mine: Json, theirs: Json): Merged {
	const conflicts: string[] = [];
	const MISSING = Symbol('missing');

	const merge = (b: Json, m: Json, t: Json, path: string): Json => {
		if (deepEqual(m, b)) return t;
		if (deepEqual(t, b) || deepEqual(m, t)) return m;
		if (isObject(m) && isObject(t)) {
			const old = isObject(b) ? b : {};
			const out: Record<string, Json> = {};
			const keys = new Set([...Object.keys(t), ...Object.keys(m)]);
			for (const key of keys) {
				const here = label(path, key);
				if (IGNORED.has(here)) {
					const value = here === 'metadata.resourceVersion' ? t[key] : m[key];
					if (value !== undefined) out[key] = value;
					continue;
				}
				const value = merge(
					key in old ? old[key] : MISSING,
					key in m ? m[key] : MISSING,
					key in t ? t[key] : MISSING,
					here
				);
				if (value !== MISSING) out[key] = value;
			}
			return out;
		}
		if (Array.isArray(m) && Array.isArray(t)) {
			const mineNamed = byName(m);
			const theirsNamed = byName(t);
			const baseNamed = Array.isArray(b) ? byName(b) : new Map<string, Json>();
			if (mineNamed && theirsNamed && baseNamed) {
				const names = [...theirsNamed.keys(), ...[...mineNamed.keys()].filter((n) => !theirsNamed.has(n))];
				return names
					.map((name) =>
						merge(
							baseNamed.get(name) ?? MISSING,
							mineNamed.get(name) ?? MISSING,
							theirsNamed.get(name) ?? MISSING,
							`${path}[name=${name}]`
						)
					)
					.filter((v) => v !== MISSING);
			}
		}
		conflicts.push(path);
		return m;
	};

	return { merged: merge(base, mine, theirs, ''), conflicts };
}

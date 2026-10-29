import { isMap, isPair, isScalar, isSeq, parseDocument, type Node } from 'yaml';

/** A schema as the API server publishes it in OpenAPI v3, trimmed by the core. */
export interface SchemaNode {
	$ref?: string;
	allOf?: SchemaNode[];
	type?: string;
	format?: string;
	description?: string;
	properties?: Record<string, SchemaNode>;
	items?: SchemaNode;
	additionalProperties?: SchemaNode | boolean;
	required?: string[];
	enum?: unknown[];
	default?: unknown;
	'x-kubernetes-preserve-unknown-fields'?: boolean;
	'x-kubernetes-int-or-string'?: boolean;
}

/** A kind's schema: the root's name and every schema it refers to. */
export interface KindSchema {
	root: string;
	schemas: Record<string, SchemaNode>;
}

/** A path into an object: keys, and indexes into lists. */
export type FieldPath = (string | number)[];

const REF = '#/components/schemas/';

/** The schema a node stands for, following `$ref` and a lone `allOf`, keeping the node's own description. */
export function resolve(schema: KindSchema, node: SchemaNode | undefined): SchemaNode | undefined {
	let current = node;
	for (let i = 0; current && i < 20; i++) {
		const ref = current.$ref ?? (current.allOf?.length === 1 ? current.allOf[0].$ref : undefined);
		if (!ref) return current;
		const target = schema.schemas[ref.replace(REF, '')];
		if (!target) return current;
		current = {
			...target,
			description: current.description ?? target.description,
			default: current.default ?? target.default
		};
	}
	return current;
}

/** The name of the schema a node refers to, such as `DeploymentSpec`. */
function refName(node: SchemaNode | undefined): string | null {
	const ref = node?.$ref ?? (node?.allOf?.length === 1 ? node.allOf[0].$ref : undefined);
	return ref ? (ref.split('.').pop() ?? null) : null;
}

/** A type as `kubectl explain` writes it: `string`, `[]Container`, `map[string]string`, `Object`. */
export function typeName(schema: KindSchema, node: SchemaNode | undefined): string {
	if (!node) return 'unknown';
	const named = refName(node);
	const resolved = resolve(schema, node);
	if (resolved?.['x-kubernetes-int-or-string']) return 'int-or-string';
	if (named && resolved?.type !== 'string' && resolved?.type !== 'integer') return named;
	if (!resolved) return 'unknown';
	if (resolved.type === 'array') return `[]${typeName(schema, resolved.items)}`;
	if (
		resolved.type === 'object' &&
		resolved.additionalProperties &&
		typeof resolved.additionalProperties === 'object'
	) {
		return `map[string]${typeName(schema, resolved.additionalProperties)}`;
	}
	if (resolved.type === 'object') return 'Object';
	return resolved.type ?? named ?? 'Object';
}

/** The schema of the field at `path`, or undefined where the schema does not say. */
export function fieldAt(schema: KindSchema, path: FieldPath): SchemaNode | undefined {
	let node: SchemaNode | undefined = schema.schemas[schema.root];
	for (const step of path) {
		const resolved = resolve(schema, node);
		if (!resolved) return undefined;
		if (typeof step === 'number') node = resolved.items;
		else if (resolved.properties?.[step]) node = resolved.properties[step];
		else if (resolved.additionalProperties && typeof resolved.additionalProperties === 'object')
			node = resolved.additionalProperties;
		else return undefined;
	}
	return node;
}

export interface FieldDoc {
	name: string;
	type: string;
	description: string;
	required: boolean;
}

/** The fields an object at `path` can have, as `kubectl explain` lists them. */
export function fieldsAt(schema: KindSchema, path: FieldPath): FieldDoc[] {
	const node = resolve(schema, fieldAt(schema, path));
	const own = node?.type === 'array' ? resolve(schema, node.items) : node;
	const required = new Set(own?.required ?? []);
	return Object.entries(own?.properties ?? {})
		.map(([name, field]) => ({
			name,
			type: typeName(schema, field),
			description: resolve(schema, field)?.description ?? '',
			required: required.has(name)
		}))
		.sort((a, b) => Number(b.required) - Number(a.required) || a.name.localeCompare(b.name));
}

/** Whether an object at `path` takes fields the schema does not list, like labels or a CRD's free-form part. */
function open(schema: KindSchema, node: SchemaNode | undefined): boolean {
	const resolved = resolve(schema, node);
	if (!resolved) return true;
	return (
		!!resolved['x-kubernetes-preserve-unknown-fields'] ||
		!!resolved.additionalProperties ||
		!resolved.properties ||
		Object.keys(resolved.properties).length === 0
	);
}

function contains(node: Node | null | undefined, offset: number): boolean {
	return !!node?.range && node.range[0] <= offset && offset <= node.range[1];
}

/** The path of the field under the cursor in a YAML text, as far as it parses. */
export function yamlPathAt(text: string, offset: number): FieldPath {
	const doc = parseDocument(text, { keepSourceTokens: false });
	const path: FieldPath = [];
	let node = doc.contents as Node | null;
	for (let depth = 0; node && depth < 100; depth++) {
		if (isMap(node)) {
			const pair = node.items.find((p) => {
				const key = p.key as Node | null;
				const value = p.value as Node | null;
				if (contains(key, offset) || contains(value, offset)) return true;
				// After the colon of a key whose value is not typed yet.
				return !value && key?.range && key.range[0] <= offset && offset <= key.range[2] + 2;
			});
			if (!pair || !isScalar(pair.key)) break;
			path.push(String(pair.key.value));
			if (contains(pair.key as Node, offset)) break;
			node = pair.value as Node | null;
		} else if (isSeq(node)) {
			const i = node.items.findIndex((item) => contains(item as Node, offset));
			if (i === -1) break;
			path.push(i);
			node = node.items[i] as Node;
		} else {
			break;
		}
	}
	return path;
}

const PLACEHOLDER = '__kubri_field__';

/**
 * Where a key being typed would go: the path of the object it belongs to and the keys it has
 * already, or null where the cursor is not at a key, as in a value or a comment.
 */
export function completionPath(
	text: string,
	offset: number
): { path: FieldPath; from: number; present: string[] } | null {
	const lineStart = text.lastIndexOf('\n', offset - 1) + 1;
	const before = text.slice(lineStart, offset);
	const match = /^(\s*(?:-\s+)*)([A-Za-z0-9_.-]*)$/.exec(before);
	if (!match) return null;
	const from = lineStart + match[1].length;
	const lineEnd = text.indexOf('\n', offset);
	const rest = lineEnd === -1 ? '' : text.slice(lineEnd);
	const probe = text.slice(0, from) + `${PLACEHOLDER}: x` + rest;
	const doc = parseDocument(probe);
	const found = findKey(doc.contents as Node | null, []);
	return found ? { ...found, from } : null;
}

function findKey(node: Node | null | undefined, path: FieldPath): { path: FieldPath; present: string[] } | null {
	if (isMap(node)) {
		for (const pair of node.items) {
			if (!isPair(pair) || !isScalar(pair.key)) continue;
			const key = String(pair.key.value);
			if (key === PLACEHOLDER) {
				const present = node.items.flatMap((p) =>
					isScalar(p.key) && p.key.value !== PLACEHOLDER ? [String(p.key.value)] : []
				);
				return { path, present };
			}
			const inner = findKey(pair.value as Node, [...path, key]);
			if (inner) return inner;
		}
	} else if (isSeq(node)) {
		for (let i = 0; i < node.items.length; i++) {
			const inner = findKey(node.items[i] as Node, [...path, i]);
			if (inner) return inner;
		}
	}
	return null;
}

export interface SchemaProblem {
	from: number;
	to: number;
	message: string;
}

/** Fields the schema does not know, where it lists every field an object may have. */
export function unknownFields(text: string, schema: KindSchema): SchemaProblem[] {
	const doc = parseDocument(text);
	if (doc.errors.length) return [];
	const problems: SchemaProblem[] = [];
	const walk = (node: Node | null | undefined, path: FieldPath) => {
		if (isMap(node)) {
			const here = fieldAt(schema, path);
			const known = resolve(schema, here)?.properties ?? {};
			const anything = open(schema, here);
			for (const pair of node.items) {
				if (!isScalar(pair.key)) continue;
				const key = String(pair.key.value);
				if (!anything && !(key in known)) {
					const range = (pair.key as Node).range;
					if (range) {
						const where = path.length ? (refName(here) ?? typeName(schema, here)) : schema.root.split('.').pop();
						problems.push({
							from: range[0],
							to: range[1],
							message: `Unknown field "${key}" in ${where}`
						});
					}
					continue;
				}
				walk(pair.value as Node, [...path, key]);
			}
		} else if (isSeq(node)) {
			node.items.forEach((item, i) => walk(item as Node, [...path, i]));
		}
	};
	walk(doc.contents as Node | null, []);
	return problems;
}

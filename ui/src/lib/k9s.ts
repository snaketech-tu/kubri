// k9s hotkeys, plugins and skins: matching their shortcuts, picking the plugins of a view and
// turning a skin into Kubri's colors.

import type { Cell, ContextInfo, ObjectRef, Plugin, ResourceInfo, ResourceRef } from './api';

export interface Shortcut {
	/** A lowercase letter, a digit or F1 to F12. */
	key: string;
	ctrl: boolean;
	shift: boolean;
	alt: boolean;
}

/** Parses a k9s shortcut such as `Ctrl-L`, `Shift-D`, `a` or `Shift-0`; null if Kubri cannot bind it. */
export function parseShortcut(text: string): Shortcut | null {
	const parts = text.trim().split('-');
	const key = parts.pop() ?? '';
	const shortcut: Shortcut = { key: '', ctrl: false, shift: false, alt: false };
	for (const modifier of parts.map((p) => p.trim().toLowerCase())) {
		if (modifier === 'ctrl' || modifier === 'control') shortcut.ctrl = true;
		else if (modifier === 'shift') shortcut.shift = true;
		else if (modifier === 'alt') shortcut.alt = true;
		else return null;
	}
	if (/^[a-z]$/i.test(key)) {
		// A bare capital is typed with Shift, as in k9s.
		if (parts.length === 0 && key !== key.toLowerCase()) shortcut.shift = true;
		shortcut.key = key.toLowerCase();
	} else if (/^[0-9]$/.test(key)) {
		shortcut.key = key;
	} else if (/^f([1-9]|1[0-2])$/i.test(key)) {
		shortcut.key = key.toUpperCase();
	} else {
		return null;
	}
	return shortcut;
}

/** Whether a key press is `shortcut`. Letters follow the keyboard layout, digits their key. */
export function matchesShortcut(e: KeyboardEvent, shortcut: Shortcut): boolean {
	if ((e.ctrlKey || e.metaKey) !== shortcut.ctrl || e.altKey !== shortcut.alt || e.shiftKey !== shortcut.shift) {
		return false;
	}
	const { key } = shortcut;
	if (/^[a-z]$/.test(key)) {
		// Alt and non-Latin layouts type other characters; then the key's position decides.
		return /^[a-z]$/i.test(e.key) ? e.key.toLowerCase() === key : e.code === `Key${key.toUpperCase()}`;
	}
	if (/^[0-9]$/.test(key)) return e.code === `Digit${key}` || (!e.shiftKey && e.key === key);
	return e.key === key;
}

/** `^l`, `⇧d`, `⌥k`, like the hints Kubri shows for its own keys. */
export function shortcutLabel(shortcut: Shortcut): string {
	return `${shortcut.ctrl ? '^' : ''}${shortcut.alt ? '⌥' : ''}${shortcut.shift ? '⇧' : ''}${shortcut.key}`;
}

/** Keys Kubri uses in at least one view; a hotkey or plugin needs `override` to take them there. */
const KUBRI_KEYS = new Set([
	...'abcdefghjklmnprsuvwy0'.split(''),
	...'AFGLNR'.split('').map((k) => `⇧${k.toLowerCase()}`),
	'^a',
	'^d',
	'^k',
	'^⇧c'
]);

export function kubriUses(shortcut: Shortcut): boolean {
	return KUBRI_KEYS.has(shortcutLabel(shortcut));
}

/** Names a plugin scope can give `resource`, as k9s accepts them. */
function scopeNames(resource: ResourceInfo): string[] {
	const names = [
		resource.plural,
		resource.singular,
		resource.kind.toLowerCase(),
		`${resource.version}/${resource.plural}`,
		...resource.shortNames
	];
	if (resource.group) names.push(`${resource.plural}.${resource.group}`, `${resource.group}/${resource.version}/${resource.plural}`);
	return names;
}

/** Where a plugin may run: a resource list, the Helm releases, or another page. */
export type PluginView = { kind: 'resource'; resource: ResourceInfo } | { kind: 'helm' } | { kind: 'other' };

const HELM_SCOPES = ['helm', 'hm', 'chart', 'charts'];

/** Whether `plugin` is offered in `view`. Container plugins run on pods, which hold the containers. */
export function pluginApplies(plugin: Plugin, view: PluginView): boolean {
	return plugin.scopes.some((raw) => {
		const scope = raw.trim().toLowerCase();
		if (scope === 'all') return true;
		if (view.kind === 'helm') return HELM_SCOPES.includes(scope);
		if (view.kind !== 'resource') return false;
		const { resource } = view;
		const pods = resource.group === '' && resource.plural === 'pods';
		return scopeNames(resource).includes(scope) || (pods && ['containers', 'container', 'co'].includes(scope));
	});
}

/**
 * Expands `$NAME`, `${NAME}`, `$COL-READY` and `${COL-%CPU/L}` from `variables` and the row's
 * cells. Unknown variables stay as written, like Kubri's plugin runner does.
 */
export function expandVariables(text: string, variables: Record<string, string>, columns: Cell[] = []): string {
	return text.replace(/\$(?:\{([^}]+)\}|(COL-[A-Za-z0-9_]+|[A-Za-z0-9_]+))/g, (match, braced, bare) => {
		const name: string = braced ?? bare;
		if (name.startsWith('COL-')) {
			const column = name.slice(4);
			const cell = /^\d+$/.test(column)
				? columns[Number(column)]
				: columns.find((c) => c.name.toLowerCase() === column.toLowerCase());
			return cell?.value ?? '';
		}
		return Object.hasOwn(variables, name) ? variables[name] : match;
	});
}

/** What the current view has under the cursor, for plugins and hotkeys. */
export interface Focus {
	/** The view's resource; null outside resource lists. */
	resource: ResourceRef | null;
	target: ObjectRef | null;
	release: { namespace: string; name: string } | null;
	/** The row's cells as shown, for `$COL-<NAME>`. */
	columns: Cell[];
	filter: string;
}

/** The plugin variables Kubri knows in the UI, for hotkey commands and confirmations. */
export function focusVariables(focus: Focus | null, context: ContextInfo | null): Record<string, string> {
	const resource = focus?.target?.resource ?? focus?.resource ?? null;
	const name = focus?.target?.name ?? focus?.release?.name ?? '';
	const pods = resource?.group === '' && resource.plural === 'pods';
	return {
		NAME: name,
		NAMESPACE: focus?.target?.namespace ?? focus?.release?.namespace ?? '',
		POD: pods ? name : '',
		FILTER: focus?.filter ?? '',
		CONTEXT: context?.name ?? '',
		CLUSTER: context?.cluster ?? '',
		USER: context?.user ?? '',
		RESOURCE_GROUP: resource?.group ?? '',
		RESOURCE_VERSION: resource?.version ?? '',
		RESOURCE_NAME: resource?.plural ?? (focus?.release ? 'helm' : '')
	};
}

/** The colors of a skin, as Kubri's CSS variables, and whether it is light or dark. */
export interface SkinTheme {
	vars: Record<string, string>;
	scheme: 'light' | 'dark';
}

/** Reads `path` from a skin, ignoring the case of keys like `highlightcolor`. */
function lookup(skin: unknown, path: string[]): unknown {
	let node = skin;
	for (const key of path) {
		if (!node || typeof node !== 'object' || Array.isArray(node)) return undefined;
		const entry = Object.entries(node).find(([k]) => k.toLowerCase() === key.toLowerCase());
		node = entry?.[1];
	}
	return node;
}

/** `#rrggbb` for hex colors; the browser resolves names like `dodgerblue`. */
export function hexColor(color: string): string | null {
	const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(color);
	if (short) return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase();
	return /^#[0-9a-f]{6}$/i.test(color) ? color.toLowerCase() : null;
}

function luminance(hex: string): number {
	const [r, g, b] = [1, 3, 5].map((i) => {
		const c = parseInt(hex.slice(i, i + 2), 16) / 255;
		return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Kubri's colors from a k9s skin. `resolve` turns a color into `#rrggbb`, or null if it is not
 * one; `default` means the terminal's color, so Kubri keeps its own. Null if the skin sets none.
 */
export function skinTheme(skin: unknown, resolve: (color: string) => string | null = hexColor): SkinTheme | null {
	const color = (...path: string[]) => {
		const value = lookup(skin, ['k9s', ...path]);
		if (typeof value !== 'string' || value.trim().toLowerCase() === 'default') return null;
		return resolve(value.trim());
	};
	const bg = color('body', 'bgColor') ?? color('views', 'table', 'bgColor');
	const text = color('body', 'fgColor') ?? color('views', 'table', 'fgColor');
	const accent = color('body', 'logoColor') ?? color('frame', 'menu', 'keyColor') ?? color('prompt', 'suggestColor');
	const entries: [string, string | null][] = [
		['--bg', bg],
		['--text', text],
		['--accent', accent],
		['--ok', color('frame', 'status', 'addColor')],
		['--warn', color('frame', 'status', 'highlightColor')],
		['--bad', color('frame', 'status', 'errorColor')],
		['--syntax-key', color('views', 'yaml', 'keyColor')],
		['--syntax-string', color('views', 'yaml', 'valueColor')]
	];
	// Dark text on a light accent, as on buttons; white has too little contrast there.
	if (accent) entries.push(['--on-accent', luminance(accent) > 0.18 ? '#10131a' : '#ffffff']);
	const vars = Object.fromEntries(entries.filter((e): e is [string, string] => e[1] !== null));
	if (Object.keys(vars).length === 0) return null;
	// Skins without a background are made for a terminal, which is usually dark.
	const scheme = bg ? (luminance(bg) > 0.4 ? 'light' : 'dark') : text && luminance(text) < 0.4 ? 'light' : 'dark';
	return { vars, scheme };
}

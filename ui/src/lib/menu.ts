/** Something to do on an object, offered as a button, in a menu and on a key. */
export interface MenuAction {
	label: string;
	/** Keys that run it, as `e.key` with `ctrl+` in front for Ctrl or Cmd. The first one is shown. */
	keys: string[];
	/** Shown instead of the first key, for keys a view handles itself, like Enter. */
	hint?: string;
	/** Where the menu lists it: actions of a group stay together, in the order given. */
	group: string;
	/** Changes or removes something that cannot simply be brought back. */
	danger?: boolean;
	/** Worth a button of its own in the action bar, which shows the lowest numbers first. */
	primary?: number;
	run: () => void;
}

/** A menu that opens at a point on the screen. */
export interface Menu {
	actions: MenuAction[];
	x: number;
	y: number;
	/** The edge of the menu the point is on: left for a click, right for a button at the end of a row. */
	align?: 'left' | 'right';
}

/** The key as a key press names it, with `ctrl+` for Ctrl or Cmd. Alt is left out on purpose. */
export function keyOf(e: KeyboardEvent): string {
	const key = e.key.length === 1 && (e.ctrlKey || e.metaKey) ? e.key.toLowerCase() : e.key;
	return e.ctrlKey || e.metaKey ? `ctrl+${key}` : key;
}

/** How a key is written in the hints: `^d`, `⇧f`, `enter`. */
export function keyLabel(key: string): string {
	if (key.startsWith('ctrl+')) return `^${keyLabel(key.slice(5))}`;
	if (key === ' ') return 'space';
	if (key === 'Delete') return 'del';
	if (key.length > 1) return key.toLowerCase();
	return key !== key.toLowerCase() ? `⇧${key.toLowerCase()}` : key;
}

/** The action a key runs: the first that lists it, as with several meanings for one key. */
export function actionFor(actions: MenuAction[], key: string): MenuAction | undefined {
	return actions.find((a) => a.keys.includes(key));
}

/** The hint for an action, or nothing when an earlier action has its key. */
export function hintOf(actions: MenuAction[], action: MenuAction): string | null {
	if (action.hint) return action.hint;
	const key = action.keys.find((k) => actionFor(actions, k) === action);
	return key === undefined ? null : keyLabel(key);
}

/** The actions in groups, in the order the groups first show up. */
export function grouped(actions: MenuAction[]): [string, MenuAction[]][] {
	const groups = new Map<string, MenuAction[]>();
	for (const action of actions) {
		const list = groups.get(action.group);
		if (list) list.push(action);
		else groups.set(action.group, [action]);
	}
	return [...groups];
}

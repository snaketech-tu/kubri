import { runHotkey, startPlugin } from './actions';
import { matchesShortcut, parseShortcut, pluginApplies, type Shortcut } from './k9s';
import { forControl } from './keys';
import { app } from './state.svelte';

export interface Binding {
	shortcut: Shortcut;
	override: boolean;
	run: () => void;
}

/** The k9s plugins of the current view, then the hotkeys, with keys Kubri can bind. */
export function bindings(): Binding[] {
	const out: Binding[] = [];
	for (const plugin of app.k9s?.plugins ?? []) {
		const shortcut = parseShortcut(plugin.shortCut);
		if (shortcut && pluginApplies(plugin, app.pluginView)) {
			out.push({ shortcut, override: plugin.override, run: () => startPlugin(plugin) });
		}
	}
	for (const hotkey of app.k9s?.hotkeys ?? []) {
		const shortcut = parseShortcut(hotkey.shortCut);
		if (shortcut) out.push({ shortcut, override: hotkey.override, run: () => runHotkey(hotkey) });
	}
	return out;
}

/**
 * Listens for plugin and hotkey keys and returns a function that stops it. Like in k9s, a key
 * the view uses stays the view's unless the plugin or hotkey says `override: true`.
 */
export function listenForBindings(): () => void {
	let pending: ((e: KeyboardEvent) => void) | null = null;
	const forget = () => {
		if (pending) window.removeEventListener('keydown', pending);
		pending = null;
	};
	const capture = (e: KeyboardEvent) => {
		forget();
		if (app.overlayOpen || e.repeat || forControl(e)) return;
		const binding = bindings().find((b) => matchesShortcut(e, b.shortcut));
		if (!binding) return;
		if (binding.override) {
			e.preventDefault();
			e.stopImmediatePropagation();
			binding.run();
			return;
		}
		// Views mark the keys they handle with preventDefault. A listener added now runs after
		// theirs, when the event bubbles back up to the window.
		pending = (late) => {
			pending = null;
			if (late !== e || e.defaultPrevented) return;
			e.preventDefault();
			binding.run();
		};
		window.addEventListener('keydown', pending, { once: true });
	};
	window.addEventListener('keydown', capture, true);
	return () => {
		window.removeEventListener('keydown', capture, true);
		forget();
	};
}

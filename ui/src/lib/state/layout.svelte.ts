import { store, stored } from '../storage';

const TOOLS_BESIDE_KEY = 'kubri.toolsBeside';
const PANEL_WIDTH_KEY = 'kubri.panelWidth';
const SIDEBAR_WIDTH_KEY = 'kubri.sidebarWidth';

/** How the window is split between the sidebar, the list and the details panel; kept for the next start. */
export class Layout {
	/** The details panel fills the window, sidebar included. */
	maximized = $state(false);
	#toolsBeside = $state(stored(TOOLS_BESIDE_KEY) === 'true');
	#panelWidth = $state(Number(stored(PANEL_WIDTH_KEY)) || null);
	#sidebarWidth = $state(Number(stored(SIDEBAR_WIDTH_KEY)) || null);

	/** Logs and shells take the place of the list, unless asked to stay beside it. */
	get toolsBeside() {
		return this.#toolsBeside;
	}
	set toolsBeside(value: boolean) {
		this.#toolsBeside = value;
		store(TOOLS_BESIDE_KEY, value ? 'true' : null);
	}

	/** The width the details panel was dragged to beside the list, or null for the default. */
	get panelWidth() {
		return this.#panelWidth;
	}
	set panelWidth(value: number | null) {
		this.#panelWidth = value;
		store(PANEL_WIDTH_KEY, value ? String(Math.round(value)) : null);
	}

	/** The width the sidebar was dragged to, or null for the default. */
	get sidebarWidth() {
		return this.#sidebarWidth;
	}
	set sidebarWidth(value: number | null) {
		this.#sidebarWidth = value;
		store(SIDEBAR_WIDTH_KEY, value ? String(Math.round(value)) : null);
	}
}

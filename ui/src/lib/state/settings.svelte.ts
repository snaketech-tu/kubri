import { backend, type ColumnSettings, type ContextSettings, type SavedSearch, type Settings } from '../api';
import { errorMessage } from '../format';
import type { AppState } from '../state.svelte';
import { store, stored } from './model';

/** Where Kubri 0.2 kept per-context choices in the browser; moved to the settings file once. */
const READ_ONLY_KEY = 'kubri.readonly.';
const ENVIRONMENT_KEY = 'kubri.environment.';

export const NO_CONTEXT_SETTINGS: ContextSettings = {
	displayName: '',
	label: '',
	color: '',
	production: false,
	productionNamespaces: [],
	productionLabels: [],
	readOnly: false,
	locked: false,
	namespace: '',
	namespaces: [],
	namespaceSelector: '',
	agentWrites: false,
	forwards: [],
	integrations: { prometheus: '', grafana: '', grafanaToken: '', loki: '', grafanaLogs: '', labels: '' }
};

const DEFAULT_SETTINGS: Settings = {
	contexts: {},
	diff: { scale: false, restart: false, image: true, labels: true },
	agents: { http: false, port: 7879, secrets: false },
	explain: { provider: 'none', baseUrl: '', model: '' },
	debug: { image: 'busybox' },
	columns: {},
	searches: [],
	history: { record: true, keepDays: 14, maxMb: 200 },
	connections: { keepRecent: 3 },
	terminal: { shell: '', args: [], startDir: '', fontSize: 13, fontFamily: '' },
	labelTemplates: []
};

/** Per-context choices Kubri 0.2 kept in the browser, as settings, for contexts that have none yet. */
export function legacyContextSettings(
	keys: string[],
	read: (key: string) => string | null,
	existing: Record<string, ContextSettings>
): Record<string, ContextSettings> {
	const found: Record<string, ContextSettings> = {};
	for (const key of keys) {
		const [prefix, name] = key.startsWith(READ_ONLY_KEY)
			? [READ_ONLY_KEY, key.slice(READ_ONLY_KEY.length)]
			: key.startsWith(ENVIRONMENT_KEY)
				? [ENVIRONMENT_KEY, key.slice(ENVIRONMENT_KEY.length)]
				: [null, ''];
		if (!prefix || !name || existing[name]) continue;
		const value = read(key) ?? '';
		const settings = (found[name] ??= { ...NO_CONTEXT_SETTINGS });
		if (prefix === READ_ONLY_KEY) settings.readOnly = value === 'true';
		else {
			settings.label = value.trim();
			settings.production = /\bprod(uction)?\b/i.test(value);
		}
	}
	return found;
}

/** Kubri's settings file: names, colors and guard rails per context, and preferences. */
export class Preferences {
	settings = $state<Settings>(DEFAULT_SETTINGS);
	/** Why the settings file could not be read. */
	error = $state<string | null>(null);

	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	contextSettingsOf(context: string | undefined | null): ContextSettings {
		return (context && this.settings.contexts[context]) || NO_CONTEXT_SETTINGS;
	}

	/** The name to show for a context: the display name set in Kubri, or its own. */
	contextName(context: string): string {
		return this.contextSettingsOf(context).displayName || context;
	}

	async load() {
		try {
			const reply = await backend.settings();
			this.settings = reply.settings;
			this.error = reply.error;
			if (!reply.error) await this.#migrateLegacy();
		} catch (err) {
			this.error = errorMessage(err);
		}
	}

	/** Moves the read-only choices and labels of Kubri 0.2 from the browser to the settings file. */
	async #migrateLegacy() {
		let keys: string[] = [];
		try {
			keys = Object.keys(localStorage).filter((k) => k.startsWith(READ_ONLY_KEY) || k.startsWith(ENVIRONMENT_KEY));
		} catch {
			return;
		}
		const found = legacyContextSettings(keys, stored, this.settings.contexts);
		for (const [name, settings] of Object.entries(found)) {
			try {
				this.settings = await backend.setContextSettings(name, settings);
			} catch {
				return;
			}
		}
		for (const key of keys) store(key, null);
	}

	/** Saves with `write`, which answers the whole settings; false, and a toast, when that fails. */
	async #save(write: () => Promise<Settings>): Promise<boolean> {
		try {
			this.settings = await write();
			return true;
		} catch (err) {
			this.#app.toast(errorMessage(err), 'bad');
			return false;
		}
	}

	/** Saves the settings of a context; the connected one's guard follows. */
	async setContextSettings(context: string, settings: ContextSettings): Promise<boolean> {
		const saved = await this.#save(() => backend.setContextSettings(context, settings));
		if (saved && context === this.#app.cluster?.context) await this.#app.rails.refresh();
		return saved;
	}

	/** Replaces the label templates for logs. */
	setLabelTemplates(templates: Settings['labelTemplates']): Promise<boolean> {
		return this.#save(() => backend.setLabelTemplates(templates));
	}

	/** Replaces the searches kept in the sidebar. */
	setSavedSearches(searches: SavedSearch[]): Promise<boolean> {
		return this.#save(() => backend.setSavedSearches(searches));
	}

	/** Saves how lists show a kind. */
	setColumnSettings(kind: string, columns: ColumnSettings): Promise<boolean> {
		return this.#save(() => backend.setColumnSettings(kind, columns));
	}

	setDiffSettings(diff: Settings['diff']): Promise<boolean> {
		return this.#save(() => backend.setDiffSettings(diff));
	}

	/** How many contexts stay connected; the switcher's marks follow. */
	async setConnectionSettings(connections: Settings['connections']): Promise<boolean> {
		const saved = await this.#save(() => backend.setConnectionSettings(connections));
		if (saved) this.#app.refreshWarm();
		return saved;
	}

	setTerminalSettings(terminal: Settings['terminal']): Promise<boolean> {
		return this.#save(() => backend.setTerminalSettings(terminal));
	}

	setHistorySettings(history: Settings['history']): Promise<boolean> {
		return this.#save(() => backend.setHistorySettings(history));
	}

	setAgentSettings(agents: Settings['agents']): Promise<boolean> {
		return this.#save(() => backend.setAgentSettings(agents));
	}

	setExplainSettings(explain: Settings['explain']): Promise<boolean> {
		return this.#save(() => backend.setExplainSettings(explain));
	}
}

import { backend } from '../api';
import { applyZoom, storedZoom } from '../appearance';
import { errorMessage } from '../format';
import { t } from '../i18n';
import { skinTheme } from '../k9s';
import { applyScheme, applyTheme, resolveColor, storeSkin, storedScheme, storedSkin, type Scheme } from '../skin';
import type { AppState } from '../state.svelte';

/** Kubri's colors, a context's k9s skin, and the desktop app's zoom. */
export class Appearance {
	/** Light or dark picked in Kubri; null follows the system. Skins pick their own. */
	scheme = $state<Scheme>(storedScheme());
	/** Whether the system is set to dark; the page follows its changes. */
	systemDark = $state(typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches);
	/** The skin picked for this context: undefined follows k9s, null is Kubri's own colors. */
	skinChoice = $state<string | null | undefined>(undefined);
	/** The skin on screen, and why the wanted one is not. */
	skin = $state<string | null>(null);
	skinError = $state<string | null>(null);
	/** Whether the skin on screen is dark. */
	skinDark = $state(false);
	/** Whether the colors on screen are dark now, a skin's or Kubri's own. */
	dark = $derived(this.skin ? this.skinDark : this.scheme ? this.scheme === 'dark' : this.systemDark);
	/** The desktop app's zoom level, kept for the next start. */
	zoom = $state(storedZoom());
	#skinRequest = 0;
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	/** Switches to Kubri's other colors; with a skin on screen, takes it off for this context. */
	toggleScheme() {
		this.setScheme(this.dark ? 'light' : 'dark');
		if (this.skin) void this.setSkin(null);
	}

	/** Light, dark, or null to follow the system. */
	setScheme(scheme: Scheme) {
		this.scheme = scheme;
		applyScheme(scheme);
	}

	setZoom(zoom: number) {
		this.zoom = zoom;
		applyZoom(zoom).catch((err) => this.#app.toast(t('Could not zoom: {error}', { error: errorMessage(err) }), 'bad'));
	}

	/** Puts on the skin picked for `context`, or its k9s skin, after its k9s files are read. */
	async loadSkin(context: string | undefined) {
		this.skinChoice = context ? storedSkin(context) : undefined;
		await this.#applySkin();
	}

	/** Picks a skin for this context: a name, null for Kubri's colors, undefined to follow k9s. */
	async setSkin(name: string | null | undefined) {
		this.skinChoice = name;
		const cluster = this.#app.cluster;
		if (cluster) storeSkin(cluster.context, name);
		await this.#applySkin();
	}

	async #applySkin() {
		const request = ++this.#skinRequest;
		const name = this.skinChoice === undefined ? (this.#app.k9s?.skin ?? null) : this.skinChoice;
		let theme = null;
		let error = null;
		if (name) {
			try {
				theme = skinTheme(await backend.k9sSkin(name), resolveColor);
				if (!theme) error = t('The skin {name} sets no colors Kubri can use.', { name });
			} catch (err) {
				error = t('Could not load the skin {name}: {error}', { name, error: errorMessage(err) });
			}
		}
		// A later choice may have finished first.
		if (request !== this.#skinRequest) return;
		applyTheme(theme);
		this.skin = theme ? name : null;
		this.skinDark = theme?.scheme === 'dark';
		this.skinError = error;
	}
}

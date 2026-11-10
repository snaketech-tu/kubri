import { backend, type FavoriteForward, type FavoriteStart, type PortForwardInfo } from '../api';
import { errorMessage } from '../format';
import { plural, t } from '../i18n';
import { coreText } from '../i18n/errors';
import type { AppState } from '../state.svelte';
import { sameResource } from './model';

/** Whether a running forward is a favorite. */
export function isForwardOf(favorite: FavoriteForward, forward: PortForwardInfo) {
	return (
		sameResource(favorite.target.resource, forward.target.resource) &&
		(favorite.target.namespace ?? null) === (forward.target.namespace ?? null) &&
		favorite.target.name === forward.target.name &&
		favorite.remotePort === forward.remotePort &&
		favorite.localPort === forward.localPort
	);
}

export function favoriteLabel(f: FavoriteForward) {
	return t('{target} on port {port}', { target: `${f.target.resource.plural}/${f.target.name}:${f.remotePort}`, port: f.localPort });
}

/** The port-forwards kept per context, to start with one click or whenever Kubri connects. */
export class FavoriteForwards {
	/** Favorites being started or stopped, so a second click waits for the first. */
	switching = $state.raw<FavoriteForward[]>([]);
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	/** Starts the connected context's favorites that start on connect, as after connecting. */
	async startAll() {
		let results: FavoriteStart[];
		try {
			results = await backend.startFavoriteForwards();
		} catch {
			return;
		}
		if (!results.length) return;
		this.#app.refreshForwards();
		const failed = results.filter((r) => r.error);
		const started = results.length - failed.length;
		if (started) this.#app.toast(plural(started, 'Started {n} favorite port-forward', 'Started {n} favorite port-forwards'));
		for (const { favorite, error } of failed) {
			this.#app.toast(t('{name} did not start: {error}', { name: favoriteLabel(favorite), error: coreText(error ?? '') }), 'bad');
		}
	}

	/** The favorite a running forward is, if it is one. */
	of(forward: PortForwardInfo): FavoriteForward | undefined {
		return this.#app.prefs.contextSettingsOf(forward.context).forwards?.find((f) => isForwardOf(f, forward));
	}

	/** The connected context's favorite port-forwards. */
	get here(): FavoriteForward[] {
		const cluster = this.#app.cluster;
		return cluster ? (this.#app.prefs.contextSettingsOf(cluster.context).forwards ?? []) : [];
	}

	/** The running forward of a favorite of the connected context, if it runs. */
	running(favorite: FavoriteForward): PortForwardInfo | undefined {
		return this.#app.forwards.find((f) => f.context === this.#app.cluster?.context && isForwardOf(favorite, f));
	}

	/** Starts a favorite of the connected context, or stops it if it runs. */
	async switch(favorite: FavoriteForward) {
		const app = this.#app;
		const context = app.cluster?.context;
		if (!context || this.switching.includes(favorite)) return;
		this.switching = [...this.switching, favorite];
		let running: PortForwardInfo | undefined;
		try {
			await app.refreshForwards();
			running = this.running(favorite);
			if (running) {
				await backend.stopPortForward(running.id);
				app.toast(t('Stopped {name}', { name: favoriteLabel(favorite) }));
			} else {
				const f = await backend.startPortForward({
					target: { ...favorite.target, context },
					remotePort: favorite.remotePort,
					localPort: favorite.localPort,
					address: favorite.address || null
				});
				app.toast(t('Forwarding {local} to {pod}', { local: `${f.address}:${f.localPort}`, pod: `${f.pod}:${f.podPort}` }));
			}
		} catch (err) {
			app.toast(running ? errorMessage(err) : t('{name} did not start: {error}', { name: favoriteLabel(favorite), error: errorMessage(err) }), 'bad');
		} finally {
			this.switching = this.switching.filter((f) => f !== favorite);
			await app.refreshForwards();
		}
	}

	/** Whether a favorite of the connected context starts whenever Kubri connects. */
	async setStartOnConnect(favorite: FavoriteForward, startOnConnect: boolean) {
		const context = this.#app.cluster?.context;
		if (!context) return;
		const settings = this.#app.prefs.contextSettingsOf(context);
		const forwards = (settings.forwards ?? []).map((f) => (f === favorite ? { ...f, startOnConnect } : f));
		await this.#app.prefs.setContextSettings(context, { ...settings, forwards });
	}

	/** Keeps a running forward as a favorite of its context, or forgets it. */
	async toggle(forward: PortForwardInfo) {
		const settings = this.#app.prefs.contextSettingsOf(forward.context);
		const kept = settings.forwards ?? [];
		const known = this.of(forward);
		const { context: _, ...target } = forward.target;
		const forwards = known
			? kept.filter((f) => f !== known)
			: [
					...kept,
					{
						target,
						remotePort: forward.remotePort,
						localPort: forward.localPort,
						address: forward.address === '127.0.0.1' ? '' : forward.address,
						startOnConnect: false
					}
				];
		if (await this.#app.prefs.setContextSettings(forward.context, { ...settings, forwards })) {
			this.#app.toast(
				known
					? t('{name} is no longer a favorite', { name: favoriteLabel(known) })
					: t('{name} is a favorite: start it with one click under Port-forwards in the sidebar', { name: favoriteLabel(forwards.at(-1)!) })
			);
		}
	}

	/** Forgets a favorite of the connected context. */
	async remove(favorite: FavoriteForward) {
		const context = this.#app.cluster?.context;
		if (!context) return;
		const settings = this.#app.prefs.contextSettingsOf(context);
		await this.#app.prefs.setContextSettings(context, { ...settings, forwards: (settings.forwards ?? []).filter((f) => f !== favorite) });
	}
}

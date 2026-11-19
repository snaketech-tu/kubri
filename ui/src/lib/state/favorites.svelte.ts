import { backend, type FavoriteForward, type FavoriteStart, type PortForwardInfo } from '../api';
import { errorMessage } from '../format';
import { plural, t } from '../i18n';
import { coreText } from '../i18n/errors';
import type { AppState } from '../state.svelte';
import { fewAtATime } from '../forward-import';
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

/** What a favorite forwards, from the target to the local port. */
export function favoriteRoute(f: FavoriteForward) {
	return t('{target} on port {port}', { target: `${f.target.resource.plural}/${f.target.name}:${f.remotePort}`, port: f.localPort });
}

/** A favorite's name, or what it forwards when it has none. */
export function favoriteLabel(f: FavoriteForward) {
	return f.name?.trim() || favoriteRoute(f);
}

/** Tells the favorites of a context apart, whatever they are named. */
export function favoriteKey(f: FavoriteForward) {
	return `${f.target.resource.group}/${f.target.resource.plural}/${f.target.namespace ?? ''}/${f.target.name}:${f.remotePort}:${f.localPort}`;
}

/** Favorites by namespace, in the order of their first favorite, for groups in lists. */
export function byNamespace(favorites: FavoriteForward[]): { namespace: string; favorites: FavoriteForward[] }[] {
	const groups = new Map<string, FavoriteForward[]>();
	for (const f of favorites) {
		const ns = f.target.namespace ?? '';
		groups.set(ns, [...(groups.get(ns) ?? []), f]);
	}
	return [...groups].map(([namespace, favorites]) => ({ namespace, favorites }));
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
		await this.#change(favorite, { startOnConnect });
	}

	/** Names a favorite of the connected context; an empty name shows its target again. */
	async rename(favorite: FavoriteForward, name: string) {
		name = name.trim();
		if (name === (favorite.name ?? '')) return true;
		return this.#change(favorite, { name });
	}

	async #change(favorite: FavoriteForward, change: Partial<FavoriteForward>) {
		const context = this.#app.cluster?.context;
		if (!context) return false;
		const settings = this.#app.prefs.contextSettingsOf(context);
		const forwards = (settings.forwards ?? []).map((f) => (f === favorite ? { ...f, ...change } : f));
		return this.#app.prefs.setContextSettings(context, { ...settings, forwards });
	}

	/** Keeps a running forward as a favorite of its context, under `name` if given, or forgets it. */
	async toggle(forward: PortForwardInfo, name = '') {
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
						name: name.trim(),
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

	/** Keeps more favorites for the connected context, after those it has. */
	async add(added: FavoriteForward[]) {
		const context = this.#app.cluster?.context;
		if (!context || !added.length) return false;
		const settings = this.#app.prefs.contextSettingsOf(context);
		return this.#app.prefs.setContextSettings(context, { ...settings, forwards: [...(settings.forwards ?? []), ...added] });
	}

	/**
	 * Starts the favorites of the connected context that do not run, or with `on` false stops
	 * those that do, and says in one message how many; each that failed has its own.
	 */
	async switchAll(favorites: FavoriteForward[], on: boolean) {
		const app = this.#app;
		const context = app.cluster?.context;
		if (!context) return;
		await app.refreshForwards();
		const todo = favorites.filter((f) => !!this.running(f) !== on && !this.switching.includes(f));
		if (!todo.length) return;
		this.switching = [...this.switching, ...todo];
		let done = 0;
		const failed: string[] = [];
		await fewAtATime(todo, async (favorite) => {
			try {
				const running = this.running(favorite);
				if (on) {
					await backend.startPortForward({
						target: { ...favorite.target, context },
						remotePort: favorite.remotePort,
						localPort: favorite.localPort,
						address: favorite.address || null
					});
				} else if (running) {
					await backend.stopPortForward(running.id);
				}
				done++;
			} catch (err) {
				failed.push(
					on ? t('{name} did not start: {error}', { name: favoriteLabel(favorite), error: errorMessage(err) }) : errorMessage(err)
				);
			} finally {
				this.switching = this.switching.filter((f) => f !== favorite);
			}
		});
		await app.refreshForwards();
		if (done) {
			app.toast(
				on
					? plural(done, 'Started {n} favorite port-forward', 'Started {n} favorite port-forwards')
					: plural(done, 'Stopped {n} favorite port-forward', 'Stopped {n} favorite port-forwards')
			);
		}
		// One message each for the first few; the rest are counted.
		for (const message of failed.slice(0, 3)) app.toast(message, 'bad');
		if (failed.length > 3) app.toast(t('{n} more did not work', { n: failed.length - 3 }), 'bad');
	}
}

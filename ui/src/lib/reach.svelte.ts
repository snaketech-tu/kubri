import { backend, type FleetCluster } from './api';
import { connectSummary } from './connect';
import { connectError, eachLimited, fleetState } from './fleet';
import { errorMessage } from './format';

/** What Kubri last learned about a context: from the Clusters page, a connect, or a quick check. */
export interface ContextReach {
	/** When it was learned. */
	at: number;
	/** The API server answered. */
	reachable: boolean;
	/** `Unreachable`, `2 pods failing`, `Healthy`. */
	summary: string;
	/** Why it could not be reached. */
	error: string | null;
	/** The API server's address, when known. */
	server: string | null;
}

/** Contexts checked at once; each gives up after ten seconds on its own. */
const AT_ONCE = 4;

/**
 * One state per context, shared by the context switcher, the Clusters page and connecting,
 * so that a context known to be down says so before anyone waits for it.
 */
class Reach {
	of = $state<Record<string, ContextReach>>({});
	#checking = new Set<string>();

	/** A Clusters page check of the context. */
	record(name: string, cluster: FleetCluster) {
		const state = fleetState(cluster);
		this.of[name] = {
			at: Date.now(),
			reachable: !!cluster.version,
			summary: cluster.version ? state.summary : connectSummary(cluster.error ?? 'timed out'),
			error: cluster.version ? null : connectError(cluster.error ?? ''),
			server: cluster.server
		};
	}

	/** Connecting failed, or the context could not be checked at all. */
	failed(name: string, error: string, server: string | null = null) {
		this.of[name] = {
			at: Date.now(),
			reachable: false,
			summary: connectSummary(error),
			error: connectError(error),
			server: server ?? this.of[name]?.server ?? null
		};
	}

	/** Kubri connected to it just now. */
	connected(name: string, server: string) {
		const before = this.of[name];
		this.of[name] = { at: Date.now(), reachable: true, summary: before?.reachable ? before.summary : 'Reachable', error: null, server };
	}

	/** Checks the contexts not checked within `maxAge`, a few at once, without connecting to them. */
	async refresh(names: string[], maxAge: number) {
		const stale = names.filter((name) => !this.#checking.has(name) && Date.now() - (this.of[name]?.at ?? 0) > maxAge);
		stale.forEach((name) => this.#checking.add(name));
		await eachLimited(stale, AT_ONCE, async (name) => {
			try {
				this.record(name, await backend.fleetCluster(name));
			} catch (err) {
				this.failed(name, errorMessage(err));
			} finally {
				this.#checking.delete(name);
			}
		});
	}
}

export const reach = new Reach();

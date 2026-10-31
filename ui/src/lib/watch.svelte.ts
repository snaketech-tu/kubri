import { untrack } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';
import { errorMessage } from './format';
import { pushTrail } from './pulse';
import { backend, type Column, type MetricsKind, type ResourceRef, type Row, type TableScope, type Usage } from './api';

/** A live table of objects. Call `start` from an `$effect` and return its result to clean up. */
export class LiveTable {
	#rows = new SvelteMap<string, Row>();
	columns = $state<Column[]>([]);
	/** Only until the first list arrives or fails; a retry with rows kept does not count. */
	loading = $state(true);
	error = $state<string | null>(null);
	/** Something the core says about the list, such as the namespaces it covers. */
	notice = $state<string | null>(null);
	rows = $derived([...this.#rows.values()]);
	/** When the last update arrived, in milliseconds; 0 before the first. */
	updated = $state(0);
	/** A list arrived since the watch began, so an empty list is truly empty. */
	hasLoaded = $state(false);
	/** What the rows shown belong to. */
	#key = '';

	/**
	 * `scope` narrows the list with selectors, as after Enter on a Deployment. `preserve` keeps the
	 * rows of the same list while the watch starts again, as after Retry; another list never shows them.
	 */
	start(resource: ResourceRef, namespace: string | null, scope: TableScope | null = null, preserve = false) {
		return untrack(() => {
			const key = JSON.stringify([resource, namespace, scope]);
			if (!preserve || this.#key !== key) {
				this.#rows.clear();
				this.columns = [];
				this.updated = 0;
				this.hasLoaded = false;
			}
			this.#key = key;
			this.loading = !this.hasLoaded;
			this.error = null;
			this.notice = null;
			// Events of a stopped watch must not reach the next one's rows.
			let stopped = false;
			const stop = backend.watchTable(resource, namespace, scope, (event) => {
				if (stopped) return;
				switch (event.type) {
					case 'reset':
						this.#rows.clear();
						for (const row of event.rows) this.#rows.set(row.key, row);
						this.columns = event.columns;
						this.loading = false;
						this.hasLoaded = true;
						break;
					case 'upsert':
						this.#rows.set(event.row.key, event.row);
						break;
					case 'delete':
						this.#rows.delete(event.row.key);
						break;
					case 'error':
						this.error = event.message;
						this.loading = false;
						return;
					case 'notice':
						this.notice = event.message;
						return;
				}
				this.updated = Date.now();
				this.error = null;
			});
			return () => {
				stopped = true;
				stop();
			};
		});
	}
}

const METRICS_INTERVAL = 15_000;

/** Polls metrics-server. `available` stays false when it is not installed. */
export class LiveMetrics {
	usage = $state(new Map<string, Usage>());
	available = $state(false);
	error = $state<string | null>(null);
	updated = $state(0);
	/** The last readings of each row, for the small trend next to CPU and memory. */
	trails = $state.raw(new Map<string, { cpu: number[]; memory: number[] }>());

	start(kind: MetricsKind, namespace: string | null) {
		this.usage = new Map();
		this.trails = new Map();
		this.available = false;
		this.error = null;
		this.updated = 0;
		let stopped = false;
		// A slow answer must not overlap the next poll.
		let polling = false;
		const poll = async () => {
			if (polling) return;
			polling = true;
			try {
				const list = await backend.metrics(kind, namespace);
				if (stopped) return;
				this.usage = new Map(list.map((u) => [`${u.namespace ?? ''}/${u.name}`, u]));
				// Rows that are gone drop their trend.
				this.trails = new Map(
					list.map((u) => {
						const key = `${u.namespace ?? ''}/${u.name}`;
						const old = this.trails.get(key);
						return [key, { cpu: pushTrail(old?.cpu, u.cpu), memory: pushTrail(old?.memory, u.memory) }];
					})
				);
				this.available = true;
				this.error = null;
				this.updated = Date.now();
			} catch (err) {
				// The last values stay, marked stale by the error.
				if (!stopped) this.error = errorMessage(err);
			} finally {
				polling = false;
			}
		};
		poll();
		const timer = setInterval(poll, METRICS_INTERVAL);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	}

	trail(namespace: string | null, name: string) {
		return this.trails.get(`${namespace ?? ''}/${name}`);
	}

	get(namespace: string | null, name: string): Usage | undefined {
		return this.usage.get(`${namespace ?? ''}/${name}`);
	}
}

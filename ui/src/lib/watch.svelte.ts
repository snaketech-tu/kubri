import { SvelteMap } from 'svelte/reactivity';
import { backend, type Column, type MetricsKind, type ResourceRef, type Row, type Usage } from './api';

/** A live table of objects. Call `start` from an `$effect` and return its result to clean up. */
export class LiveTable {
	#rows = new SvelteMap<string, Row>();
	columns = $state<Column[]>([]);
	loading = $state(true);
	error = $state<string | null>(null);
	rows = $derived([...this.#rows.values()]);

	start(resource: ResourceRef, namespace: string | null) {
		this.#rows.clear();
		this.columns = [];
		this.loading = true;
		this.error = null;
		return backend.watchTable(resource, namespace, (event) => {
			switch (event.type) {
				case 'reset':
					this.#rows.clear();
					for (const row of event.rows) this.#rows.set(row.key, row);
					this.columns = event.columns;
					this.loading = false;
					this.error = null;
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
					break;
			}
		});
	}
}

const METRICS_INTERVAL = 15_000;

/** Polls metrics-server. `available` stays false when it is not installed. */
export class LiveMetrics {
	usage = $state(new Map<string, Usage>());
	available = $state(false);

	start(kind: MetricsKind, namespace: string | null) {
		this.usage = new Map();
		this.available = false;
		let stopped = false;
		const poll = async () => {
			try {
				const list = await backend.metrics(kind, namespace);
				if (stopped) return;
				this.usage = new Map(list.map((u) => [`${u.namespace ?? ''}/${u.name}`, u]));
				this.available = true;
			} catch {
				if (!stopped) this.available = false;
			}
		};
		poll();
		const timer = setInterval(poll, METRICS_INTERVAL);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	}

	get(namespace: string | null, name: string): Usage | undefined {
		return this.usage.get(`${namespace ?? ''}/${name}`);
	}
}

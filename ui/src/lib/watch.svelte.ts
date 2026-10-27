import { SvelteMap } from 'svelte/reactivity';
import { backend, type KubeObject, type ResourceRef } from './api';

function key(obj: KubeObject) {
	return obj.metadata.uid ?? `${obj.metadata.namespace ?? ''}/${obj.metadata.name}`;
}

/** A live list of objects. Call `start` from an `$effect` and return its result to clean up. */
export class LiveList {
	#items = new SvelteMap<string, KubeObject>();
	loading = $state(true);
	error = $state<string | null>(null);

	items = $derived(
		[...this.#items.values()].sort(
			(a, b) =>
				(a.metadata.namespace ?? '').localeCompare(b.metadata.namespace ?? '') ||
				a.metadata.name.localeCompare(b.metadata.name)
		)
	);

	start(resource: ResourceRef, namespace: string | null) {
		this.#items.clear();
		this.loading = true;
		this.error = null;
		return backend.watch(resource, namespace, (event) => {
			switch (event.type) {
				case 'reset':
					this.#items.clear();
					for (const item of event.items) this.#items.set(key(item), item);
					this.loading = false;
					this.error = null;
					break;
				case 'upsert':
					this.#items.set(key(event.item), event.item);
					break;
				case 'delete':
					this.#items.delete(key(event.item));
					break;
				case 'error':
					this.error = event.message;
					this.loading = false;
					break;
			}
		});
	}
}

import { backend, type KindView, type ResourceInfo, type ViewSet } from '../api';

/** How custom resource kinds show: columns, health, details and actions from YAML files. */
export class KindViews {
	loaded = $state<ViewSet | null>(null);

	/** Reads the view files again, so an edited one shows on the next connect or reload. */
	async load() {
		try {
			this.loaded = await backend.views();
		} catch {
			// Without them, custom resources show the server's columns.
		}
	}

	/** The view of a kind, if one is loaded. */
	of(resource: Pick<ResourceInfo, 'group' | 'kind'>): KindView | undefined {
		return this.loaded?.views.find((v) => v.group === resource.group && v.kind === resource.kind);
	}
}

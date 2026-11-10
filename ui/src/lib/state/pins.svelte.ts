import type { ObjectRef, ResourceInfo } from '../api';
import { t } from '../i18n';
import { loadPins, savePins, samePin, toggled, type Pin } from '../pins';
import { recentObjects, remember, type RecentObject } from '../search';
import type { AppState } from '../state.svelte';
import { findResource, refOf } from './model';

/** Kinds and objects kept in the sidebar, and objects opened lately. */
export class Pins {
	/** Objects opened lately, newest first, in any context. */
	recent = $state.raw<RecentObject[]>(recentObjects());
	/** The pins of every context; the sidebar shows the current one's. */
	all = $state.raw<Pin[]>(loadPins());
	#app: AppState;

	constructor(app: AppState) {
		this.#app = app;
	}

	/** The pins of the connected context, in the order they were made. */
	get here(): Pin[] {
		const context = this.#app.cluster?.context;
		return context ? this.all.filter((p) => p.context === context) : [];
	}

	/** The pin of a kind, or of an object of it, in the connected context. */
	of(resource: ResourceInfo, target?: { namespace: string | null; name: string }): Pin | null {
		const context = this.#app.cluster?.context;
		if (!context) return null;
		return {
			context,
			kind: resource.kind,
			...refOf(resource),
			namespace: target?.namespace ?? null,
			name: target?.name ?? ''
		};
	}

	has(pin: Pin | null): boolean {
		return !!pin && this.all.some((p) => samePin(p, pin));
	}

	toggle(pin: Pin | null) {
		if (!pin) return;
		const was = this.has(pin);
		this.all = toggled(this.all, pin);
		savePins(this.all);
		this.#app.toast(was ? t('Unpinned {name}', { name: pin.name || pin.kind }) : t('Pinned {name} to the sidebar', { name: pin.name || pin.kind }));
	}

	/** Puts an object first among the recent ones of the connected context. */
	opened(resource: ResourceInfo, target: ObjectRef) {
		const cluster = this.#app.cluster;
		if (!cluster) return;
		this.recent = remember({
			context: cluster.context,
			kind: resource.kind,
			...refOf(resource),
			namespace: target.namespace,
			name: target.name
		});
	}

	/** Opens a pin or a recent object: the list of a kind, a namespace's pods, or an object in its list. */
	open(pin: RecentObject) {
		const app = this.#app;
		const resource = findResource(app.resources, { group: pin.group, version: pin.version, plural: pin.plural });
		if (!resource) {
			app.toast(t('The cluster does not serve {kind}', { kind: pin.plural }), 'bad');
			return;
		}
		if (!pin.name) {
			app.open(resource);
		} else if (resource.plural === 'namespaces' && resource.group === '') {
			const pods = findResource(app.resources, { group: '', version: 'v1', plural: 'pods' });
			if (pods) app.open(pods, pin.name, '');
		} else {
			app.openAt(resource, pin.namespace, pin.name);
		}
	}
}

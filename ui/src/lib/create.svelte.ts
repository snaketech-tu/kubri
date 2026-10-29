import { backend, type KubeObject, type ResourceInfo } from './api';
import { cloneable, skeleton, templateObject, toYaml, type TemplateParams } from './create';

/** The YAML on the Create page, kept while you look elsewhere. */
class Creating {
	text = $state('');
	/** Counts replacements from outside the editor, such as a template, so the editor takes them. */
	version = $state(0);
	/** For objects that name no namespace; empty for the namespace in view. */
	namespace = $state('');
	/** The context to apply in; null for the connected one. */
	into = $state<string | null>(null);
	/** Where the YAML came from, such as the object a clone started from. */
	source = $state<string | null>(null);

	set(text: string) {
		this.text = text;
		this.version++;
	}

	/** Starts over with a copy of `object`, for another namespace or context. */
	clone(object: KubeObject, context: string | null) {
		const meta = object.metadata ?? {};
		this.set(toYaml([cloneable(object as Record<string, any>)]));
		this.namespace = meta.namespace ?? '';
		this.into = context;
		this.source = `${object.kind} ${meta.namespace ? `${meta.namespace}/` : ''}${meta.name}`;
	}

	/** Starts from a template for `resource`, or from what its schema requires. */
	async start(resource: ResourceInfo, params: TemplateParams) {
		const builtIn = templateObject(resource.kind, params);
		const group = String(builtIn?.apiVersion ?? '')
			.split('/')
			.slice(0, -1)
			.join('/');
		if (builtIn && group === resource.group) {
			this.set(toYaml([builtIn]));
			return;
		}
		const schema = await backend.schema(resource).catch(() => null);
		this.set(toYaml([skeleton(resource, schema, params)]));
	}
}

export const creating = new Creating();

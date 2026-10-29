import { backend, type ResourceInfo } from './api';
import { skeleton, templateObject, toYaml, type TemplateParams } from './create';

/** The YAML on the Create page, kept while you look elsewhere. */
class Creating {
	text = $state('');
	/** Counts replacements from outside the editor, such as a template, so the editor takes them. */
	version = $state(0);

	set(text: string) {
		this.text = text;
		this.version++;
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

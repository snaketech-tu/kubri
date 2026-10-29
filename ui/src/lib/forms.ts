import { splitWords } from './create';

/** A path into an object: keys, and indexes into lists. */
export type Path = (string | number)[];

export function getPath(object: unknown, path: Path): unknown {
	let at: any = object;
	for (const step of path) {
		if (at == null || typeof at !== 'object') return undefined;
		at = at[step];
	}
	return at;
}

/** Sets the value at `path`, making maps and lists on the way; `undefined` removes it. */
export function setPath(object: Record<string, any>, path: Path, value: unknown) {
	let at: any = object;
	for (let i = 0; i < path.length - 1; i++) {
		const step = path[i];
		if (at[step] == null || typeof at[step] !== 'object') {
			if (value === undefined) return;
			at[step] = typeof path[i + 1] === 'number' ? [] : {};
		}
		at = at[step];
	}
	const last = path[path.length - 1];
	if (value === undefined) {
		if (Array.isArray(at) && typeof last === 'number') at.splice(last, 1);
		else delete at[last];
	} else {
		at[last] = value;
	}
}

export type FieldKind = 'text' | 'number' | 'select' | 'map' | 'command';

export interface FormField {
	label: string;
	path: Path;
	kind: FieldKind;
	options?: string[];
	placeholder?: string;
	hint?: string;
}

const name: FormField = { label: 'Name', path: ['metadata', 'name'], kind: 'text' };
const pod = (base: Path): FormField[] => [
	{ label: 'Image', path: [...base, 'containers', 0, 'image'], kind: 'text', placeholder: 'nginx:1.27' },
	{
		label: 'Command',
		path: [...base, 'containers', 0, 'command'],
		kind: 'command',
		placeholder: 'sh -c "echo hello"',
		hint: 'Instead of the image’s own command; empty runs the image as it is.'
	},
	{ label: 'Port', path: [...base, 'containers', 0, 'ports', 0, 'containerPort'], kind: 'number' },
	{
		label: 'CPU request',
		path: [...base, 'containers', 0, 'resources', 'requests', 'cpu'],
		kind: 'text',
		placeholder: '100m'
	},
	{
		label: 'Memory limit',
		path: [...base, 'containers', 0, 'resources', 'limits', 'memory'],
		kind: 'text',
		placeholder: '256Mi'
	}
];

/** The guided forms, by kind. Each writes into the same YAML the editor shows. */
export const FORMS: Record<string, FormField[]> = {
	Deployment: [
		name,
		{ label: 'Replicas', path: ['spec', 'replicas'], kind: 'number' },
		...pod(['spec', 'template', 'spec'])
	],
	Service: [
		name,
		{ label: 'Type', path: ['spec', 'type'], kind: 'select', options: ['ClusterIP', 'NodePort', 'LoadBalancer'] },
		{
			label: 'Selector',
			path: ['spec', 'selector'],
			kind: 'map',
			hint: 'The labels of the pods that get the traffic.'
		},
		{ label: 'Port', path: ['spec', 'ports', 0, 'port'], kind: 'number' },
		{
			label: 'Target port',
			path: ['spec', 'ports', 0, 'targetPort'],
			kind: 'text',
			hint: 'A number, or the name of a container port.'
		}
	],
	Job: [
		name,
		{ label: 'Retries', path: ['spec', 'backoffLimit'], kind: 'number' },
		...pod(['spec', 'template', 'spec']).filter((f) => f.label !== 'Port')
	],
	CronJob: [
		name,
		{
			label: 'Schedule',
			path: ['spec', 'schedule'],
			kind: 'text',
			placeholder: '0 3 * * *',
			hint: 'Cron syntax: minute, hour, day, month, weekday.'
		},
		{ label: 'Suspended', path: ['spec', 'suspend'], kind: 'select', options: ['false', 'true'] },
		...pod(['spec', 'jobTemplate', 'spec', 'template', 'spec']).filter((f) => f.label !== 'Port')
	],
	ConfigMap: [name, { label: 'Data', path: ['data'], kind: 'map' }],
	Secret: [
		name,
		{
			label: 'Type',
			path: ['type'],
			kind: 'select',
			options: ['Opaque', 'kubernetes.io/basic-auth', 'kubernetes.io/tls', 'kubernetes.io/dockerconfigjson']
		},
		{ label: 'Values', path: ['stringData'], kind: 'map', hint: 'Plain text; the API server stores it base64-encoded.' }
	]
};

/** The value of a field as its input shows it. */
export function fieldText(object: unknown, field: FormField): string {
	const value = getPath(object, field.path);
	if (value == null) return '';
	if (field.kind === 'command' && Array.isArray(value)) {
		return value.map((w) => (/[\s"']/.test(String(w)) ? JSON.stringify(String(w)) : String(w))).join(' ');
	}
	return String(value);
}

/** The value to store for what was typed into a field; `undefined` removes it. */
export function fieldValue(field: FormField, text: string): unknown {
	const trimmed = text.trim();
	if (!trimmed) return undefined;
	switch (field.kind) {
		case 'number': {
			const n = Number(trimmed);
			return Number.isFinite(n) ? n : trimmed;
		}
		case 'command':
			return splitWords(trimmed);
		case 'select':
			return trimmed === 'true' ? true : trimmed === 'false' ? false : trimmed;
		case 'text':
			// A target port may be a number or a name.
			return /^\d+$/.test(trimmed) && field.path.at(-1) === 'targetPort' ? Number(trimmed) : trimmed;
	}
	return trimmed;
}

/**
 * Renames an object, and for workloads the `app` label and container name where the template
 * set them to the name, so the selector and the pod labels keep matching.
 */
export function rename(object: Record<string, any>, to: string) {
	const from = object.metadata?.name;
	setPath(object, ['metadata', 'name'], to || undefined);
	const labelPaths: Path[] = [
		['spec', 'selector', 'matchLabels', 'app'],
		['spec', 'template', 'metadata', 'labels', 'app'],
		['spec', 'jobTemplate', 'spec', 'template', 'metadata', 'labels', 'app'],
		['spec', 'template', 'spec', 'containers', 0, 'name'],
		['spec', 'jobTemplate', 'spec', 'template', 'spec', 'containers', 0, 'name']
	];
	for (const path of labelPaths) {
		if (from && to && getPath(object, path) === from) setPath(object, path, to);
	}
}

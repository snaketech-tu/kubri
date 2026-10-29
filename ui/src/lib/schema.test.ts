import { describe, expect, it } from 'vitest';
import { completionPath, fieldsAt, typeName, fieldAt, unknownFields, yamlPathAt, type KindSchema } from './schema';

const ref = (name: string) => ({
	allOf: [{ $ref: `#/components/schemas/${name}` }]
});

const schema: KindSchema = {
	root: 'apps.v1.Deployment',
	schemas: {
		'apps.v1.Deployment': {
			type: 'object',
			properties: {
				apiVersion: { type: 'string' },
				kind: { type: 'string' },
				metadata: {
					...ref('meta.v1.ObjectMeta'),
					description: 'Standard object metadata.'
				},
				spec: {
					...ref('apps.v1.DeploymentSpec'),
					description: 'Desired behavior.'
				}
			}
		},
		'meta.v1.ObjectMeta': {
			type: 'object',
			properties: {
				name: { type: 'string', description: 'Name must be unique.' },
				labels: { type: 'object', additionalProperties: { type: 'string' } }
			}
		},
		'apps.v1.DeploymentSpec': {
			type: 'object',
			required: ['template'],
			properties: {
				replicas: { type: 'integer', description: 'Number of desired pods.' },
				template: ref('core.v1.PodTemplateSpec'),
				extra: { type: 'object', 'x-kubernetes-preserve-unknown-fields': true }
			}
		},
		'core.v1.PodTemplateSpec': {
			type: 'object',
			properties: { spec: ref('core.v1.PodSpec') }
		},
		'core.v1.PodSpec': {
			type: 'object',
			properties: {
				containers: { type: 'array', items: ref('core.v1.Container') }
			}
		},
		'core.v1.Container': {
			type: 'object',
			required: ['name'],
			properties: {
				name: { type: 'string' },
				image: { type: 'string', description: 'Container image name.' },
				ports: {
					type: 'array',
					items: {
						type: 'object',
						properties: { containerPort: { type: 'integer' } }
					}
				}
			}
		}
	}
};

const yaml = `apiVersion: apps/v1
kind: Deployment
metadata:
  name: web
  labels:
    app: web
spec:
  replicas: 2
  extra:
    anything: goes
  template:
    spec:
      containers:
        - name: web
          image: busybox
`;

describe('schema', () => {
	it('names types like kubectl explain', () => {
		expect(typeName(schema, fieldAt(schema, ['spec']))).toBe('DeploymentSpec');
		expect(typeName(schema, fieldAt(schema, ['spec', 'template', 'spec', 'containers']))).toBe('[]Container');
		expect(typeName(schema, fieldAt(schema, ['metadata', 'labels']))).toBe('map[string]string');
		expect(typeName(schema, fieldAt(schema, ['spec', 'replicas']))).toBe('integer');
		expect(fieldAt(schema, ['spec', 'nope'])).toBeUndefined();
	});

	it('keeps the description of the field over that of its type', () => {
		expect(fieldsAt(schema, []).find((f) => f.name === 'spec')?.description).toBe('Desired behavior.');
	});

	it('lists fields, required first', () => {
		expect(fieldsAt(schema, ['spec']).map((f) => f.name)).toEqual(['template', 'extra', 'replicas']);
		expect(fieldsAt(schema, ['spec', 'template', 'spec', 'containers']).map((f) => f.name)).toEqual([
			'name',
			'image',
			'ports'
		]);
	});

	it('finds the path under the cursor', () => {
		expect(yamlPathAt(yaml, yaml.indexOf('busybox') + 2)).toEqual([
			'spec',
			'template',
			'spec',
			'containers',
			0,
			'image'
		]);
		expect(yamlPathAt(yaml, yaml.indexOf('replicas') + 1)).toEqual(['spec', 'replicas']);
		expect(yamlPathAt(yaml, yaml.indexOf('app: web'))).toEqual(['metadata', 'labels', 'app']);
	});

	it('finds where a key being typed goes', () => {
		const typing = 'spec:\n  template:\n    spec:\n      containers:\n        - na';
		expect(completionPath(typing, typing.length)).toEqual({
			path: ['spec', 'template', 'spec', 'containers', 0],
			from: typing.length - 2,
			present: []
		});
		const top = 'spec:\n  replicas: 1\n  re';
		expect(completionPath(top, top.length)).toEqual({
			path: ['spec'],
			from: top.length - 2,
			present: ['replicas']
		});
		const value = 'spec:\n  replicas: 2';
		expect(completionPath(value, value.length)).toBeNull();
	});

	it('flags fields the schema does not know, but not in free-form objects', () => {
		const typo = yaml.replace('image: busybox', 'imag: busybox').replace('replicas: 2', 'replicas: 2\n  bogus: 1');
		expect(unknownFields(typo, schema).map((p) => p.message)).toEqual([
			'Unknown field "bogus" in DeploymentSpec',
			'Unknown field "imag" in Container'
		]);
		expect(unknownFields(yaml, schema)).toEqual([]);
		expect(unknownFields('spec: [unclosed', schema)).toEqual([]);
	});
});

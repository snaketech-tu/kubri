import { describe, expect, it } from 'vitest';
import type { ResourceInfo } from './api';
import { parseObjects, parseShortForm, skeleton, templateObject, toYaml, workloadOf } from './create';
import type { KindSchema } from './schema';

describe('templates', () => {
	it('fills a Deployment from a short form', () => {
		const form = parseShortForm('deploy web --image nginx:1.27 --replicas 3 --port=8080 -n shop')!;
		expect(form.kind).toBe('deploy');
		const object = templateObject('Deployment', form.params) as any;
		expect(object.metadata).toEqual({ name: 'web', namespace: 'shop' });
		expect(object.spec.replicas).toBe(3);
		expect(object.spec.selector.matchLabels).toEqual({ app: 'web' });
		expect(object.spec.template.spec.containers[0]).toEqual({
			name: 'web',
			image: 'nginx:1.27',
			ports: [{ containerPort: 8080 }]
		});
	});

	it('reads service types, literals, schedules and commands', () => {
		const svc = parseShortForm('svc nodeport web --tcp 80:8080')!;
		expect((templateObject('Service', svc.params) as any).spec).toEqual({
			type: 'NodePort',
			selector: { app: 'web' },
			ports: [{ name: 'http', port: 80, targetPort: 8080 }]
		});
		const secret = parseShortForm('secret generic db --from-literal password=a=b --from-literal=user=app')!;
		expect(secret.params).toEqual({ name: 'db', data: { password: 'a=b', user: 'app' } });
		const cron = parseShortForm('cronjob nightly --image busybox --schedule "0 3 * * *" -- sh -c date')!;
		const job = templateObject('CronJob', cron.params) as any;
		expect(job.spec.schedule).toBe('0 3 * * *');
		expect(job.spec.jobTemplate.spec.template.spec.containers[0].command).toEqual(['sh', '-c', 'date']);
	});

	it('leaves the namespace out of cluster-scoped kinds and has none for unknown kinds', () => {
		expect(templateObject('Namespace', { name: 'team', namespace: 'shop' })).toEqual({
			apiVersion: 'v1',
			kind: 'Namespace',
			metadata: { name: 'team' }
		});
		expect(templateObject('Widget')).toBeNull();
	});

	it('builds other kinds from what their schema requires', () => {
		const resource = {
			group: 'example.com',
			version: 'v1',
			plural: 'widgets',
			kind: 'Widget',
			namespaced: true
		} as ResourceInfo;
		const schema: KindSchema = {
			root: 'Widget',
			schemas: {
				Widget: { type: 'object', required: ['spec'], properties: { spec: { $ref: '#/components/schemas/Spec' } } },
				Spec: {
					type: 'object',
					required: ['size', 'colors', 'mode', 'owner'],
					properties: {
						size: { type: 'integer' },
						colors: { type: 'array', items: { type: 'string' } },
						mode: { type: 'string', enum: ['fast', 'slow'] },
						owner: { type: 'object', required: ['name'], properties: { name: { type: 'string' } } }
					}
				}
			}
		};
		expect(skeleton(resource, schema, { name: 'w', namespace: 'shop' })).toEqual({
			apiVersion: 'example.com/v1',
			kind: 'Widget',
			metadata: { name: 'w', namespace: 'shop' },
			spec: { size: 0, colors: [], mode: 'fast', owner: { name: '' } }
		});
		expect(skeleton(resource, null)).toEqual({
			apiVersion: 'example.com/v1',
			kind: 'Widget',
			metadata: { name: 'my-widget' },
			spec: {}
		});
	});
});

describe('related objects', () => {
	it('points a Service at the last workload', () => {
		const web = templateObject('Deployment', { name: 'web', port: 8080 });
		expect(workloadOf([web, { kind: 'ConfigMap', metadata: { name: 'cfg' } }])).toEqual({ name: 'web', port: 8080 });
		expect(workloadOf([{ kind: 'ConfigMap', metadata: { name: 'cfg' } }])).toEqual({});
	});
});

describe('parsing YAML', () => {
	it('reads several documents and Lists, and skips empty ones', () => {
		const text = toYaml([{ kind: 'A' }, { kind: 'List', items: [{ kind: 'B' }, { kind: 'C' }] }]) + '---\n';
		expect(parseObjects(text)).toEqual({ objects: [{ kind: 'A' }, { kind: 'B' }, { kind: 'C' }], error: null });
	});

	it('says where the YAML is broken', () => {
		const { error } = parseObjects('kind: A\n---\nkind: [B\n');
		expect(error).toMatch(/^Line \d+: /);
	});
});

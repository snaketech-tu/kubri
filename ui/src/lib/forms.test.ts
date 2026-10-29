import { describe, expect, it } from 'vitest';
import { templateObject } from './create';
import { FORMS, fieldText, fieldValue, getPath, rename, setPath } from './forms';

const field = (kind: string, label: string) => FORMS[kind].find((f) => f.label === label)!;

describe('guided forms', () => {
	it('sets and removes values along a path', () => {
		const object: Record<string, any> = {};
		setPath(object, ['spec', 'containers', 0, 'ports', 0, 'containerPort'], 80);
		expect(object).toEqual({ spec: { containers: [{ ports: [{ containerPort: 80 }] }] } });
		setPath(object, ['spec', 'containers', 0, 'ports', 0], undefined);
		expect(getPath(object, ['spec', 'containers', 0, 'ports'])).toEqual([]);
		setPath(object, ['metadata', 'labels', 'a'], undefined);
		expect(object.metadata).toBeUndefined();
	});

	it('reads and writes commands, numbers and named ports', () => {
		const deploy = templateObject('Deployment', { name: 'web', command: ['sh', '-c', 'echo hi'] })!;
		const command = field('Deployment', 'Command');
		expect(fieldText(deploy, command)).toBe('sh -c "echo hi"');
		expect(fieldValue(command, 'sh -c "echo hi"')).toEqual(['sh', '-c', 'echo hi']);
		expect(fieldValue(field('Deployment', 'Replicas'), '3')).toBe(3);
		expect(fieldValue(field('Service', 'Target port'), '8080')).toBe(8080);
		expect(fieldValue(field('Service', 'Target port'), 'http')).toBe('http');
		expect(fieldValue(field('CronJob', 'Suspended'), 'true')).toBe(true);
		expect(fieldValue(field('Deployment', 'Image'), '  ')).toBeUndefined();
	});

	it('renames a workload with its app label and container', () => {
		const deploy = templateObject('Deployment', { name: 'web' }) as Record<string, any>;
		deploy.spec.template.metadata.labels.tier = 'web';
		rename(deploy, 'shop');
		expect(deploy.metadata.name).toBe('shop');
		expect(deploy.spec.selector.matchLabels).toEqual({ app: 'shop' });
		expect(deploy.spec.template.metadata.labels).toEqual({ app: 'shop', tier: 'web' });
		expect(deploy.spec.template.spec.containers[0].name).toBe('shop');
		const svc = templateObject('Service', { name: 'web' }) as Record<string, any>;
		rename(svc, 'web-svc');
		// A Service keeps pointing at the pods it selected.
		expect(svc.spec.selector).toEqual({ app: 'web' });
	});
});

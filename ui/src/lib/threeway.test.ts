import { describe, expect, it } from 'vitest';
import { changedMeaningfully, threeWay } from './threeway';

const base = {
	metadata: { name: 'web', resourceVersion: '1', labels: { app: 'web' } },
	spec: {
		replicas: 2,
		template: {
			spec: {
				containers: [
					{ name: 'web', image: 'web:1' },
					{ name: 'sidecar', image: 'proxy:1' }
				]
			}
		}
	},
	status: { replicas: 2 }
};

const copy = () => structuredClone(base);

describe('threeWay', () => {
	it('applies my edit onto theirs where we changed different fields', () => {
		const mine = copy();
		mine.spec.replicas = 3;
		const theirs = copy();
		theirs.metadata.resourceVersion = '7';
		theirs.metadata.labels = {
			app: 'web',
			team: 'shop'
		} as typeof theirs.metadata.labels;
		theirs.spec.template.spec.containers[1].image = 'proxy:2';
		const { merged, conflicts } = threeWay(base, mine, theirs);
		expect(conflicts).toEqual([]);
		expect(merged).toEqual({
			...theirs,
			spec: { ...theirs.spec, replicas: 3 }
		});
	});

	it('merges named list items one by one, and keeps mine where both changed a field', () => {
		const mine = copy();
		mine.spec.template.spec.containers[0].image = 'web:2';
		mine.spec.template.spec.containers.push({
			name: 'debug',
			image: 'busybox'
		});
		const theirs = copy();
		theirs.spec.template.spec.containers[0].image = 'web:3';
		theirs.spec.template.spec.containers.splice(1, 1);
		const { merged, conflicts } = threeWay(base, mine, theirs);
		expect(conflicts).toEqual(['spec.template.spec.containers[name=web].image']);
		expect((merged as typeof base).spec.template.spec.containers).toEqual([
			{ name: 'web', image: 'web:2' },
			{ name: 'debug', image: 'busybox' }
		]);
	});

	it('takes their resourceVersion and keeps my fields being removed', () => {
		const mine = copy() as Record<string, unknown> & typeof base;
		delete (mine.metadata as Partial<typeof base.metadata>).labels;
		const theirs = copy();
		theirs.metadata.resourceVersion = '9';
		const { merged } = threeWay(base, mine, theirs);
		expect((merged as typeof base).metadata).toEqual({
			name: 'web',
			resourceVersion: '9'
		});
	});

	it('tells status-only updates from real changes', () => {
		const theirs = copy();
		theirs.metadata.resourceVersion = '5';
		theirs.status.replicas = 1;
		expect(changedMeaningfully(base, theirs)).toBe(false);
		theirs.spec.replicas = 5;
		expect(changedMeaningfully(base, theirs)).toBe(true);
	});
});

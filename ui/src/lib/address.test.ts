import { describe, expect, it } from 'vitest';
import { fromHash, toHash } from './address';

const pages = ['resources', 'pulses', 'xray'] as const;
type Page = (typeof pages)[number];
const deployments = { group: 'apps', version: 'v1', plural: 'deployments' };

describe('addresses of views', () => {
	it('writes a view as kubectl names kinds and reads it back', () => {
		const view = {
			context: 'arn:aws:eks:eu-west-1:1:cluster/prod',
			page: 'resources' as Page,
			resource: deployments,
			namespace: 'shop',
			scope: { title: 'Pods of Deployment web', scope: { labelSelector: 'app=web' }, ownerUid: null },
			xray: null
		};
		const hash = toHash(view);
		expect(hash.startsWith('#/arn%3Aaws%3Aeks%3Aeu-west-1%3A1%3Acluster%2Fprod/resources/deployments.v1.apps?ns=shop&scope=')).toBe(true);
		expect(fromHash(hash, pages)).toEqual(view);
	});

	it('leaves out what a view does not have', () => {
		const pods = { group: '', version: 'v1', plural: 'pods' };
		const hash = toHash({ context: 'lab', page: 'xray' as Page, resource: null, namespace: '', scope: null, xray: pods });
		expect(hash).toBe('#/lab/xray?xray=pods.v1');
		expect(fromHash(hash, pages)).toEqual({ context: 'lab', page: 'xray', resource: null, namespace: '', scope: null, xray: pods });
		expect(toHash({ context: 'lab', page: 'pulses' as Page, resource: null, namespace: '', scope: null, xray: null })).toBe('#/lab/pulses');
	});

	it('ignores addresses it did not write', () => {
		expect(fromHash('', pages)).toBeNull();
		expect(fromHash('#section', pages)).toBeNull();
		expect(fromHash('#/lab/nowhere', pages)).toBeNull();
		expect(fromHash('#/%E0%A4%A/pulses', pages)).toBeNull();
		expect(fromHash('#/lab/resources/pods.v1?scope=%7Bbroken', pages)?.scope).toBeNull();
		expect(fromHash('#/lab/resources/pods', pages)?.resource).toBeNull();
	});
});

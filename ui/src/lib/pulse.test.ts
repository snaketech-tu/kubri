import { describe, expect, it } from 'vitest';
import type { KindPulse, RecentEvent } from './api';
import { attentionItems, shortMessage, splitKinds } from './pulse';

const pods = { group: '', version: 'v1', plural: 'pods' };

function event(name: string, reason: string, minutesAgo: number, count = 1): RecentEvent {
	return {
		kind: 'Warning',
		reason,
		message: `${reason} for ${name}`,
		count,
		source: 'kubelet',
		firstSeen: null,
		lastSeen: new Date(Date.UTC(2026, 9, 29, 12, 0) - minutesAgo * 60_000).toISOString(),
		objectKind: 'Pod',
		object: { resource: pods, namespace: 'shop', name }
	};
}

describe('attention items', () => {
	it('groups the warnings of one object into one item, with the latest event first', () => {
		const items = attentionItems([
			event('emails', 'Unhealthy', 6, 3),
			event('emails', 'BackOff', 2, 8),
			event('emails', 'BackOff', 9, 1)
		]);
		expect(items).toHaveLength(1);
		expect(items[0].event.reason).toBe('BackOff');
		expect(items[0].reasons).toEqual(['BackOff', 'Unhealthy']);
		expect(items[0].count).toBe(12);
		expect(items[0].severity).toBe('critical');
	});

	it('puts outages before warnings that may resolve on their own', () => {
		const items = attentionItems([event('inventory', 'FailedScheduling', 1), event('emails', 'BackOff', 5)]);
		expect(items.map((i) => [i.event.object.name, i.severity])).toEqual([
			['emails', 'critical'],
			['inventory', 'warning']
		]);
	});

	it('keeps objects of the same name in different namespaces apart', () => {
		const other = { ...event('emails', 'BackOff', 1), object: { resource: pods, namespace: 'mail', name: 'emails' } };
		expect(attentionItems([event('emails', 'BackOff', 2), other])).toHaveLength(2);
	});
});

describe('shortMessage', () => {
	it('says a crash loop plainly', () => {
		expect(
			shortMessage('BackOff', 'Back-off restarting failed container checkout in pod checkout-7d_shop(0f3a-uuid)', 26)
		).toBe('Container checkout keeps restarting (26× in the last hour)');
	});
	it('names the image that cannot be pulled', () => {
		expect(shortMessage('Failed', 'Failed to pull image "ghcr.io/x/y:1": rpc error: not found', 1)).toBe(
			'Cannot pull image ghcr.io/x/y:1'
		);
	});
	it('keeps the node count of a scheduling failure', () => {
		expect(shortMessage('FailedScheduling', '0/3 nodes are available: 3 Insufficient cpu.', 1)).toBe(
			'Cannot be scheduled: 0/3 nodes are available'
		);
	});
	it('cuts unknown texts at the first sentence', () => {
		expect(shortMessage('Other', 'Something odd happened. More detail follows here.', 1)).toBe('Something odd happened');
		expect(shortMessage('Other', 'x'.repeat(300), 1).length).toBe(138);
		expect(shortMessage('Failed', 'Error: ErrImagePull', 1)).toBe('Error: ErrImagePull');
	});
});

describe('splitKinds', () => {
	const kind = (name: string, ok: number, warning = 0, failed = 0, error: string | null = null): KindPulse => ({
		kind: name,
		resource: pods,
		total: ok + warning + failed,
		ok,
		warning,
		failed,
		error
	});
	it('puts kinds with problems first and folds the rest', () => {
		const { problems, healthy } = splitKinds([kind('A', 3), kind('B', 1, 0, 1), kind('C', 2), kind('D', 0, 0, 0, 'forbidden')]);
		expect(problems.map((k) => k.kind)).toEqual(['B', 'D']);
		expect(healthy.map((k) => k.kind)).toEqual(['A', 'C']);
	});
});

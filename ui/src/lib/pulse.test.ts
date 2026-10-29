import { describe, expect, it } from 'vitest';
import type { RecentEvent } from './api';
import { attentionItems } from './pulse';

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

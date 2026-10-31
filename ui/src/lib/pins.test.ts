import { describe, expect, it } from 'vitest';
import { samePin, toggled, type Pin } from './pins';

const pin = (name: string, context = 'a'): Pin => ({
	context,
	kind: 'Pod',
	group: '',
	version: 'v1',
	plural: 'pods',
	namespace: 'shop',
	name
});

describe('pins', () => {
	it('adds a pin, and removes it when toggled again', () => {
		const one = toggled([], pin('web'));
		expect(one).toHaveLength(1);
		expect(toggled(one, pin('web'))).toEqual([]);
	});
	it('tells contexts apart', () => {
		expect(samePin(pin('web', 'a'), pin('web', 'b'))).toBe(false);
	});
	it('keeps at most thirty, dropping the oldest', () => {
		let pins: Pin[] = [];
		for (let i = 0; i < 35; i++) pins = toggled(pins, pin(`p${i}`));
		expect(pins).toHaveLength(30);
		expect(pins[0].name).toBe('p5');
	});
});

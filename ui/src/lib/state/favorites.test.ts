import { describe, expect, it } from 'vitest';
import type { FavoriteForward } from '../api';
import { byNamespace, favoriteKey, favoriteLabel, favoriteRoute } from './favorites.svelte';

const favorite = (name = '', localPort = 8080): FavoriteForward => ({
	target: { resource: { group: '', version: 'v1', plural: 'services' }, namespace: 'shop', name: 'web' },
	name,
	remotePort: 80,
	localPort,
	address: '',
	startOnConnect: false
});

describe('favorite port-forwards', () => {
	it('show their name, or what they forward without one', () => {
		expect(favoriteLabel(favorite())).toBe('services/web:80 on port 8080');
		expect(favoriteLabel(favorite('  '))).toBe('services/web:80 on port 8080');
		expect(favoriteLabel(favorite('Shop'))).toBe('Shop');
		expect(favoriteRoute(favorite('Shop'))).toBe('services/web:80 on port 8080');
	});
	it('stay apart by what they forward, whatever they are named', () => {
		expect(favoriteKey(favorite('Shop'))).toBe(favoriteKey(favorite('Other')));
		expect(favoriteKey(favorite('Shop', 8080))).not.toBe(favoriteKey(favorite('Shop', 8081)));
	});
	it('group by namespace in the order they were kept', () => {
		const at = (namespace: string, name: string): FavoriteForward => ({ ...favorite(), target: { ...favorite().target, namespace, name } });
		const [a, b, c] = [at('shop', 'a'), at('pay', 'b'), at('shop', 'c')];
		expect(byNamespace([a, b, c])).toEqual([
			{ namespace: 'shop', favorites: [a, c] },
			{ namespace: 'pay', favorites: [b] }
		]);
	});
});

import { describe, expect, it } from 'vitest';
import type { Request } from './generated/Request';
import { callBackend } from './rpc';

describe('callBackend', () => {
	it('sends each call by name with its arguments next to it', async () => {
		const sent: Request[] = [];
		const backend = callBackend(async (request) => {
			sent.push(request);
			return request.call === 'cancelSwitch' ? null : { ok: request.call };
		});
		expect(await backend.contexts()).toEqual({ ok: 'contexts' });
		await backend.addContexts('kind: Config', ['one']);
		await backend.removeContext('one');
		await backend.switchContext('two');
		expect(await backend.cancelSwitch()).toBeUndefined();
		expect(sent).toEqual([
			{ call: 'contexts' },
			{ call: 'addContexts', text: 'kind: Config', names: ['one'] },
			{ call: 'removeContext', name: 'one' },
			{ call: 'switchContext', name: 'two' },
			{ call: 'cancelSwitch' }
		]);
	});

	it('sends request objects next to the call and fills left-out arguments with null', async () => {
		const sent: Request[] = [];
		const backend = callBackend(async (request) => {
			sent.push(request);
			return null;
		});
		const target = { resource: { group: '', version: 'v1', plural: 'pods' }, namespace: 'shop', name: 'web' };
		await backend.startPortForward({ target, remotePort: 80 });
		await backend.deleteObject(target);
		await backend.helmRelease('shop', 'web');
		await backend.access([]);
		expect(await backend.scale(target, 2)).toBeUndefined();
		expect(sent).toEqual([
			{ call: 'startPortForward', target, remotePort: 80 },
			{ call: 'deleteObject', target, options: null },
			{ call: 'helmRelease', namespace: 'shop', name: 'web', revision: null },
			{ call: 'access', checks: [], fresh: false },
			{ call: 'scale', target, replicas: 2 }
		]);
	});
});

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
});

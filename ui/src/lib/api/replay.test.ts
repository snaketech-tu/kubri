import { describe, expect, it } from 'vitest';
import { answer, apiPath, canonical, index, missingSocket, NOT_RECORDED, socketKey, type Recording } from './replay';

const recording: Recording = {
	version: 1,
	http: [
		{ method: 'GET', url: 'http://127.0.0.1:7899/api/object?name=web&namespace=shop&plural=pods', body: null, status: 200, type: 'application/json', response: '{"kind":"Pod"}' },
		{ method: 'POST', url: '/api/call', body: '{"call":"access","checks":[{"verb":"get"}],"fresh":false}', status: 200, type: 'application/json', response: '[true]' },
		{ method: 'POST', url: '/api/call', body: '{"call":"history","until":1}', status: 200, type: 'application/json', response: '{"entries":[]}' },
		{ method: 'POST', url: '/api/call', body: '{"call":"settings"}', status: 200, type: 'application/json', response: '{"settings":{"theme":"dark"},"error":null}' }
	],
	sockets: [
		{ url: 'ws://127.0.0.1:7899/api/table?plural=pods&namespace=shop', first: null, messages: [{ text: '{"type":"rows"}' }], closed: false },
		{ url: 'ws://127.0.0.1:7899/api/logs', first: '{"target":{"name":"web"},"options":{"tail":100}}', messages: [{ text: '{"type":"end"}' }], closed: true }
	]
};

describe('the live demo', () => {
	const replay = index(recording);

	it('sorts keys and query parameters, so equal requests match', () => {
		expect(canonical({ b: 1, a: [{ d: 2, c: 3 }], e: undefined })).toBe('{"a":[{"c":3,"d":2}],"b":1}');
		expect(apiPath(new URL('https://example.github.io/api/object?plural=pods&name=web&namespace=shop'))).toBe(
			'/object?name=web&namespace=shop&plural=pods'
		);
		expect(apiPath(new URL('https://example.github.io/kubri/demo/index.html'))).toBeNull();
	});

	it('answers what was recorded, wherever the page is', () => {
		expect(answer(replay, 'GET', '/object?name=web&namespace=shop&plural=pods').body).toBe('{"kind":"Pod"}');
		expect(answer(replay, 'post', '/call', '{"fresh":false,"checks":[{"verb":"get"}],"call":"access"}').body).toBe('[true]');
	});

	it('says what it cannot show', () => {
		const missing = answer(replay, 'GET', '/object?name=api&namespace=shop&plural=pods');
		expect(missing.status).toBe(501);
		expect(JSON.parse(missing.body).error).toBe(NOT_RECORDED);
		expect(answer(replay, 'POST', '/call', '{"call":"access","checks":[],"fresh":false}').status).toBe(501);
	});

	it('answers requests that differ by time with any recorded one', () => {
		expect(answer(replay, 'POST', '/call', '{"call":"history","until":2}').body).toBe('{"entries":[]}');
		// Settings changed in the demo stay as they were.
		const columns = answer(replay, 'POST', '/call', '{"call":"setColumnSettings","kind":"Pod","columns":{}}');
		expect(JSON.parse(columns.body)).toEqual({ theme: 'dark' });
		expect(answer(replay, 'POST', '/call', '{"call":"setSavedSearches","searches":[]}').status).toBe(200);
		expect(answer(replay, 'POST', '/call', '{"call":"scale","replicas":2}').status).toBe(501);
	});

	it('plays sockets by their address and first request', () => {
		expect(replay.sockets.get(socketKey('/table?namespace=shop&plural=pods'))?.messages).toHaveLength(1);
		const logs = replay.sockets.get(socketKey('/logs', '{"options":{"tail":100},"target":{"name":"web"}}'));
		expect(logs?.closed).toBe(true);
		expect(missingSocket('/logs').messages.map((m) => ('text' in m ? JSON.parse(m.text).type : null))).toEqual(['error', 'end']);
		expect(missingSocket('/table?plural=nodes').closed).toBe(false);
	});
});

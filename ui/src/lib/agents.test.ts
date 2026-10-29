import { describe, expect, it } from 'vitest';
import { callArguments, httpSnippets, stdioSnippets } from './agents';

const connection = {
	url: 'http://127.0.0.1:7879/mcp',
	error: null,
	token: 'abc',
	command: ['/Applications/Kubri.app/Contents/MacOS/kubri', '--mcp']
};

describe('agent snippets', () => {
	it('starts Kubri with --mcp over stdio', () => {
		const [code, desktop] = stdioSnippets(connection);
		expect(code.text).toBe('claude mcp add kubri -- /Applications/Kubri.app/Contents/MacOS/kubri --mcp');
		expect(JSON.parse(desktop.text)).toEqual({
			mcpServers: { kubri: { command: '/Applications/Kubri.app/Contents/MacOS/kubri', args: ['--mcp'] } }
		});
	});

	it('quotes paths with spaces', () => {
		const [code] = stdioSnippets({ ...connection, command: ['C:\\Program Files\\Kubri\\kubri.exe', '--mcp'] });
		expect(code.text).toBe("claude mcp add kubri -- 'C:\\Program Files\\Kubri\\kubri.exe' --mcp");
	});

	it('sends the token to the HTTP endpoint', () => {
		const [code, other] = httpSnippets(connection, 'abc');
		expect(code.text).toBe(
			"claude mcp add --transport http kubri http://127.0.0.1:7879/mcp --header 'Authorization: Bearer abc'"
		);
		expect(JSON.parse(other.text).mcpServers.kubri.headers.Authorization).toBe('Bearer abc');
		expect(httpSnippets({ ...connection, url: null }, 'abc')).toEqual([]);
	});

	it('writes arguments in one line', () => {
		expect(callArguments({ resource: 'pods', limit: 5, wide: true })).toBe('resource=pods limit=5 wide=true');
	});
});

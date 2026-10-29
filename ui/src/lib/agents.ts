import type { AgentConnection } from './api';
import { shellQuote } from './kubectl';

export interface AgentSnippet {
	/** Where it goes, such as "Claude Code". */
	client: string;
	/** What to run or paste, with the token in place. */
	text: string;
	/** For a configuration file rather than a command. */
	json: boolean;
}

const HIDDEN = '•'.repeat(12);

/** The token as shown on screen: hidden unless asked. */
export function shownToken(token: string, visible: boolean) {
	return visible ? token : HIDDEN;
}

/** How to connect common MCP clients to Kubri over stdio, which only reads. */
export function stdioSnippets(connection: AgentConnection): AgentSnippet[] {
	const [command, ...args] = connection.command;
	return [
		{
			client: 'Claude Code',
			text: ['claude', 'mcp', 'add', 'kubri', '--', command, ...args].map(shellQuote).join(' '),
			json: false
		},
		{
			client: 'Claude Desktop, Cursor and other clients',
			text: JSON.stringify({ mcpServers: { kubri: { command, args } } }, null, 2),
			json: true
		}
	];
}

/** How to connect common MCP clients to Kubri's HTTP endpoint, where changes are possible. */
export function httpSnippets(connection: AgentConnection, token: string): AgentSnippet[] {
	if (!connection.url) return [];
	const auth = `Authorization: Bearer ${token}`;
	return [
		{
			client: 'Claude Code',
			text: ['claude', 'mcp', 'add', '--transport', 'http', 'kubri', connection.url, '--header', auth]
				.map(shellQuote)
				.join(' '),
			json: false
		},
		{
			client: 'Other clients',
			text: JSON.stringify(
				{ mcpServers: { kubri: { type: 'http', url: connection.url, headers: { Authorization: `Bearer ${token}` } } } },
				null,
				2
			),
			json: true
		}
	];
}

/** A tool call's arguments in one line, such as `resource=pods namespace=shop`. */
export function callArguments(args: Record<string, unknown>): string {
	return Object.entries(args ?? {})
		.map(([key, value]) => `${key}=${typeof value === 'string' ? value : JSON.stringify(value)}`)
		.join(' ');
}

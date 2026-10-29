<script lang="ts">
	import { isTauri } from '@tauri-apps/api/core';
	import { callArguments, httpSnippets, shownToken, stdioSnippets, type AgentSnippet } from '../agents';
	import { backend, type AgentCall, type AgentConnection } from '../api';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';

	let connection = $state<AgentConnection | null>(null);
	let calls = $state<AgentCall[]>([]);
	let showToken = $state(false);
	let way = $state<'http' | 'stdio'>('http');
	const desktop = isTauri();

	const agents = $derived(app.settings.agents);
	const writable = $derived(
		app.contexts.filter((c) => {
			const s = app.contextSettingsOf(c.name);
			return s.agentWrites && !s.production && !s.readOnly;
		})
	);
	const token = $derived(connection?.token ?? 'TOKEN');
	const snippets = $derived<AgentSnippet[]>(
		!connection ? [] : way === 'http' ? httpSnippets(connection, token) : stdioSnippets(connection)
	);

	async function refresh() {
		try {
			[connection, calls] = await Promise.all([backend.agentConnection(), backend.agentCalls()]);
		} catch {
			// Shown as missing; the settings page still works.
		}
	}

	$effect(() => {
		refresh();
		const timer = setInterval(refresh, 5000);
		return () => clearInterval(timer);
	});

	async function update(change: Partial<typeof agents>) {
		await app.setAgentSettings({ ...agents, ...change });
		await refresh();
	}

	async function newToken() {
		try {
			connection = await backend.newAgentToken();
			app.toast('New token made: agents using the old one must be set up again');
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
	}

	function copy(text: string, what: string) {
		navigator.clipboard.writeText(text).then(
			() => app.toast(`Copied ${what}`),
			(err) => app.toast(errorMessage(err), 'bad')
		);
	}

	/** What a snippet looks like on screen, with the token hidden unless shown. */
	function display(snippet: AgentSnippet) {
		return connection?.token ? snippet.text.replaceAll(connection.token, shownToken(connection.token, showToken)) : snippet.text;
	}

	function time(seconds: number) {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<div class="block">
	<h2 id="config-agents">AI agents</h2>
	<p class="muted">
		Kubri is an MCP server: AI agents such as Claude Code, Claude Desktop or an IDE assistant can read your clusters
		through it, with your permissions. Secrets stay hidden and every call is logged here.
	</p>

	<div class="ways" role="tablist" aria-label="How agents connect">
		<button role="tab" aria-selected={way === 'http'} class:active={way === 'http'} onclick={() => (way = 'http')}>
			Through this Kubri (HTTP)
		</button>
		<button role="tab" aria-selected={way === 'stdio'} class:active={way === 'stdio'} onclick={() => (way = 'stdio')}>
			The agent starts Kubri (stdio)
		</button>
	</div>

	{#if way === 'http'}
		<label class="check">
			<input type="checkbox" checked={agents.http} onchange={(e) => update({ http: e.currentTarget.checked })} />
			<span>
				<strong>Serve MCP on this computer</strong>
				<small>
					While Kubri runs, agents connect to it with a token. Changes are possible only on contexts that allow them, and
					you allow each one in this window.
				</small>
			</span>
		</label>
		{#if desktop}
			<label class="port">
				<span>Port</span>
				<input
					type="number"
					min="1024"
					max="65535"
					value={agents.port}
					onchange={(e) => {
						const port = Number(e.currentTarget.value);
						if (port >= 1024 && port <= 65535) update({ port });
						else app.toast('Pick a port from 1024 to 65535', 'bad');
					}}
				/>
			</label>
		{/if}
		{#if connection?.error}
			<p class="error">{connection.error}</p>
		{/if}
		{#if agents.http && connection?.url}
			<div class="endpoint">
				<code>{connection.url}</code>
				{#if connection.token}
					<span class="muted">token</span>
					<code>{shownToken(connection.token, showToken)}</code>
					<button class="small" onclick={() => (showToken = !showToken)}>{showToken ? 'Hide' : 'Show'}</button>
					<button class="small" onclick={() => connection?.token && copy(connection.token, 'the token')}>Copy</button>
					<button class="small" onclick={newToken}>New token</button>
				{/if}
			</div>
			<p class="muted small">
				{#if writable.length}
					Agents may change {writable.map((c) => app.contextName(c.name)).join(', ')}, after you allow it.
				{:else}
					Agents only read: no context allows changes. Turn on “AI agents may change it” for a context above to allow them.
				{/if}
			</p>
		{/if}
	{:else}
		<p class="muted small">
			The agent runs Kubri itself and talks to it over stdin and stdout. Nothing needs to be turned on, but since no
			Kubri window can confirm a change this way, agents only read.
		</p>
	{/if}

	{#if snippets.length}
		{#each snippets as snippet (snippet.client)}
			<div class="snippet">
				<div class="snippet-head">
					<span>{snippet.client}</span>
					<button class="small" onclick={() => copy(snippet.text, snippet.json ? 'the configuration' : 'the command')}>
						Copy
					</button>
				</div>
				<pre>{display(snippet)}</pre>
			</div>
		{/each}
	{/if}

	<label class="check">
		<input type="checkbox" checked={agents.secrets} onchange={(e) => update({ secrets: e.currentTarget.checked })} />
		<span>
			<strong>Agents may read Secret values</strong>
			<small>Otherwise they see a Secret's keys and sizes, never its values.</small>
		</span>
	</label>

	<h3>Recent tool calls</h3>
	{#if calls.length}
		<div class="calls">
			<table>
				<thead>
					<tr><th>When</th><th>Tool</th><th>Arguments</th><th>Context</th><th>Result</th></tr>
				</thead>
				<tbody>
					{#each calls.slice(0, 50) as call, i (i)}
						<tr>
							<td class="nowrap">{time(call.time)}</td>
							<td><code>{call.tool}</code> <span class="muted small">{call.transport}</span></td>
							<td class="args"><code>{callArguments(call.arguments)}</code></td>
							<td>{call.context ? app.contextName(call.context) : '–'}</td>
							<td class:bad={call.error}>{call.error ?? `ok, ${call.millis} ms`}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<p class="muted small">No agent has called Kubri yet.</p>
	{/if}
</div>

<style>
	.block {
		margin-bottom: 14px;
		padding: 16px 18px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 15px;
	}
	h3 {
		margin: 18px 0 8px;
		font-size: 13px;
	}
	p {
		margin: 0 0 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	.error {
		color: var(--bad);
	}
	.ways {
		display: inline-flex;
		margin-bottom: 14px;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.ways button {
		padding: 6px 12px;
		border: none;
		background: var(--surface-1);
		font-size: 13px;
	}
	.ways button + button {
		border-left: 1px solid var(--border);
	}
	.ways button.active {
		background: var(--accent);
		color: var(--on-accent);
	}
	.check {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px;
		align-items: start;
		margin: 0 0 12px;
		font-size: 13px;
	}
	.check input {
		margin-top: 3px;
	}
	.check span {
		display: grid;
		gap: 1px;
	}
	.check small {
		color: var(--text-muted);
		font-size: 12px;
	}
	.port {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0 0 12px 24px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.port input {
		width: 90px;
	}
	.endpoint {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin: 0 0 8px 24px;
		font-size: 12.5px;
	}
	.endpoint + p {
		margin-left: 24px;
	}
	code,
	pre {
		font-family: var(--mono);
	}
	button.small {
		padding: 2px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.snippet {
		margin: 0 0 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.snippet-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 5px 10px;
		background: var(--surface-2);
		font-size: 12px;
		color: var(--text-muted);
	}
	pre {
		margin: 0;
		padding: 8px 12px;
		overflow: auto;
		font-size: 12px;
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.calls {
		max-height: 320px;
		overflow: auto;
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
	}
	th,
	td {
		padding: 5px 10px;
		border-bottom: 1px solid var(--border-subtle);
		text-align: left;
		vertical-align: top;
	}
	th {
		position: sticky;
		top: 0;
		background: var(--surface-2);
		font-weight: 600;
	}
	.nowrap {
		white-space: nowrap;
	}
	.args {
		max-width: 320px;
		overflow-wrap: anywhere;
	}
	td.bad {
		color: var(--bad);
	}
</style>

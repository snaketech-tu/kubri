<script lang="ts">
	import { backend, type AgentRequest } from '../api';
	import { app } from '../state.svelte';

	let requests = $state<AgentRequest[]>([]);
	let box = $state<HTMLDialogElement>();
	let answering = $state(false);
	let refuse = $state<HTMLButtonElement>();

	const request = $derived(requests[0] ?? null);

	// While agents can connect, ask for changes they wait on. Asking also tells the MCP server
	// that a window is open to confirm them.
	$effect(() => {
		if (!app.settings.agents.http) {
			requests = [];
			return;
		}
		let stopped = false;
		const poll = async () => {
			try {
				const next = await backend.agentRequests();
				if (!stopped) requests = next;
			} catch {
				// The next poll tries again.
			}
		};
		poll();
		const timer = setInterval(poll, 2000);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	});

	$effect(() => {
		if (request && box && !box.open) {
			box.showModal();
			// Refusing is the safe answer to a stray Enter.
			refuse?.focus();
		}
		if (!request && box?.open) box.close();
	});

	async function answer(allow: boolean) {
		if (!request) return;
		answering = true;
		const id = request.id;
		try {
			await backend.answerAgent(id, allow);
		} finally {
			answering = false;
			requests = requests.filter((r) => r.id !== id);
		}
		app.toast(allow ? 'Allowed the change' : 'Refused the change');
		if (allow) app.refreshGuard();
	}
</script>

<dialog
	class="dialog"
	aria-label="An AI agent asks to make a change"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		answer(false);
	}}
>
	{#if request}
		{@const settings = app.contextSettingsOf(request.context)}
		<h2>An AI agent asks to make a change</h2>
		<p class="summary">{request.summary}</p>
		<p class="muted">
			On
			<strong style:color={settings.color || null}>{request.context ? app.contextName(request.context) : 'the current context'}</strong
			>{#if settings.label}&nbsp;<span class="tag">{settings.label}</span>{/if}. Nothing happens unless you allow it.
		</p>
		<details>
			<summary>Same as kubectl</summary>
			<pre>{request.kubectl}</pre>
		</details>
		{#if requests.length > 1}
			<p class="muted small">{requests.length - 1} more {requests.length === 2 ? 'change waits' : 'changes wait'} after this one.</p>
		{/if}
		<div class="buttons">
			<button class="secondary" bind:this={refuse} onclick={() => answer(false)} disabled={answering}>Refuse</button>
			<button class="primary" onclick={() => answer(true)} disabled={answering}>Allow</button>
		</div>
	{/if}
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 60;
		width: min(520px, calc(100vw - 32px));
		margin: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	.summary {
		font-size: 15px;
		font-weight: 600;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	.tag {
		padding: 1px 6px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 11px;
	}
	details {
		margin: 0 0 12px;
		font-size: 12px;
	}
	pre {
		margin: 6px 0 0;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--surface-2);
		font-family: var(--mono);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}
	.buttons button {
		padding: 8px 14px;
		border-radius: 8px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
</style>

<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type ExplainAnswer, type ExplainPrompt, type ExplainSubject } from '../api';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';
	import { explainBlocker, providerName } from '../explain';
	import Icon from './Icon.svelte';
	import Markdown from './Markdown.svelte';

	let { subject }: { subject: ExplainSubject } = $props();

	let box = $state<HTMLDialogElement>();
	let send = $state<HTMLButtonElement>();
	let prompt = $state<ExplainPrompt | null>(null);
	let answer = $state<ExplainAnswer | null>(null);
	let error = $state<string | null>(null);
	let phase = $state<'gathering' | 'preview' | 'asking' | 'answered'>('gathering');
	/** Answers of a closed panel are dropped. */
	let open = true;

	const blocker = $derived(prompt ? explainBlocker(prompt) : null);

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		untrack(gather);
		return () => {
			open = false;
			previous?.focus();
		};
	});

	async function gather() {
		phase = 'gathering';
		error = null;
		try {
			prompt = await backend.explainPrompt(subject);
			phase = 'preview';
			// Sending is one key away once the prompt is read, never before.
			setTimeout(() => send?.focus());
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function ask() {
		if (!prompt || blocker) return;
		phase = 'asking';
		error = null;
		try {
			const reply = await backend.explain(prompt.text);
			if (!open) return;
			answer = reply;
			phase = 'answered';
		} catch (err) {
			if (!open) return;
			error = errorMessage(err);
			phase = 'preview';
		}
	}

	function copy() {
		if (!answer) return;
		navigator.clipboard.writeText(answer.text).then(
			() => app.toast('Copied the answer'),
			(err) => app.toast(errorMessage(err), 'bad')
		);
	}

	function close() {
		app.explaining = null;
	}

	function settings() {
		close();
		app.showConfig('explain');
	}
</script>

<dialog
	class="panel"
	aria-label="Explain"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
	onclick={(e) => {
		if (e.target === box) close();
	}}
>
	<header>
		<div>
			<h2>Explain</h2>
			<p class="muted">{prompt?.title ?? 'Gathering what Kubri knows…'}</p>
		</div>
		<button class="icon" onclick={close} aria-label="Close" title="Close (Esc)"><Icon name="close" size={16} /></button>
	</header>

	<div class="body">
		{#if phase === 'gathering' && !error}
			<p class="muted">Gathering the object, its events and logs…</p>
		{/if}

		{#if blocker}
			<div class="notice">
				<p>{blocker}</p>
				<button class="secondary" onclick={settings}>Open Explain settings</button>
			</div>
		{/if}

		{#if error}
			<p class="error" role="alert">{error}</p>
		{/if}

		{#if phase === 'asking'}
			<p class="waiting" role="status">
				<span class="spinner" aria-hidden="true"></span>
				Asking {prompt?.model}… this can take a minute.
			</p>
		{/if}

		{#if answer && phase === 'answered'}
			<section class="answer" aria-label="Answer">
				<Markdown text={answer.text} />
				<p class="meta">
					Answered by {answer.model}{answer.cutShort ? '. The answer was cut short at the length limit.' : '.'} Models can be
					wrong: check before you change anything.
				</p>
			</section>
		{/if}

		{#if prompt}
			<details class="sent" open={phase !== 'answered'}>
				<summary>{phase === 'answered' ? 'What was sent' : 'What will be sent'}</summary>
				<p class="muted small">
					{#if prompt.endpoint}
						To <strong>{providerName(prompt.provider)}</strong>{prompt.model ? `, model ${prompt.model}` : ''}, at
						<code>{prompt.endpoint}</code>.
					{/if}
					Secret values, tokens, passwords and keys are hidden as <code>&lt;hidden&gt;</code>. Nothing is sent before you
					press Send.
				</p>
				<pre>{prompt.text}</pre>
				<details class="system">
					<summary>Instructions for the model</summary>
					<pre>{prompt.system}</pre>
				</details>
			</details>
		{/if}
	</div>

	<footer>
		{#if phase === 'answered' && answer}
			<button class="secondary" onclick={copy}>Copy answer</button>
			<button class="secondary" onclick={gather}>Ask again</button>
			<button class="primary" onclick={close}>Close</button>
		{:else}
			<button class="secondary" onclick={close}>Cancel</button>
			{#if error && !prompt}
				<button class="primary" onclick={gather}>Try again</button>
			{:else}
				<button class="primary" bind:this={send} onclick={ask} disabled={!prompt || !!blocker || phase !== 'preview'}>
					{error ? 'Send again' : 'Send'}
				</button>
			{/if}
		{/if}
	</footer>
</dialog>

<style>
	.panel {
		position: fixed;
		inset: 0 0 0 auto;
		z-index: 70;
		display: flex;
		flex-direction: column;
		width: min(600px, 100vw);
		max-width: 100vw;
		height: 100dvh;
		max-height: 100dvh;
		margin: 0;
		padding: 0;
		border: none;
		border-left: 1px solid var(--border);
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
	}
	.panel::backdrop {
		background: rgb(16 21 34 / 25%);
	}
	header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 16px 20px 12px;
		border-bottom: 1px solid var(--border);
	}
	h2 {
		margin: 0 0 2px;
		font-size: 17px;
	}
	header p {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.icon {
		padding: 4px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
	}
	.body {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 16px 20px;
	}
	footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 20px;
		border-top: 1px solid var(--border);
	}
	footer button {
		padding: 8px 14px;
		border-radius: 8px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled {
		opacity: 0.5;
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	.error {
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.notice {
		margin: 0 0 14px;
		padding: 12px 14px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-2);
	}
	.notice p {
		margin: 0 0 10px;
	}
	.notice button {
		padding: 5px 10px;
		border-radius: 6px;
	}
	.waiting {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-muted);
	}
	.spinner {
		width: 14px;
		height: 14px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.9s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spinner {
			animation: none;
		}
	}
	.answer {
		margin: 0 0 16px;
	}
	.meta {
		margin: 12px 0 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	.sent > summary,
	.system > summary {
		cursor: pointer;
		font-size: 13px;
		font-weight: 600;
	}
	.sent > p {
		margin: 8px 0;
	}
	.system {
		margin-top: 10px;
	}
	pre {
		max-height: 50dvh;
		margin: 8px 0 0;
		padding: 10px 12px;
		overflow: auto;
		border-radius: 8px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12px;
		line-height: 1.45;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
	}
</style>

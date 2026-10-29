<script lang="ts">
	import { backend, type NewContext } from '../api';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';

	let { onclose }: { onclose: (added: string[]) => void } = $props();

	let box = $state<HTMLDialogElement>();
	let picker = $state<HTMLInputElement>();
	let text = $state('');
	let source = $state('');
	let found = $state<NewContext[]>([]);
	let chosen = $state<string[]>([]);
	let busy = $state(false);
	let dragging = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	async function inspect() {
		error = null;
		found = [];
		chosen = [];
		if (!text.trim()) return;
		try {
			found = await backend.inspectKubeconfig(text);
			chosen = found.filter((c) => !c.problem).map((c) => c.name);
			if (!found.length) error = 'This kubeconfig has no contexts.';
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function read(file: File | undefined) {
		if (!file) return;
		if (file.size > 1024 * 1024) {
			error = `${file.name} is larger than a kubeconfig should be.`;
			return;
		}
		text = await file.text();
		source = file.name;
		await inspect();
	}

	async function add() {
		busy = true;
		error = null;
		try {
			const list = await backend.addContexts(text, chosen);
			app.contexts = list.contexts;
			app.toast(chosen.length === 1 ? `Added ${chosen[0]}` : `Added ${chosen.length} contexts`);
			onclose(chosen);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}
</script>

<dialog
	class="dialog"
	aria-label="Add clusters"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		if (!busy) onclose([]);
	}}
>
	<h2>Add clusters from a kubeconfig</h2>
	<p class="lead">
		Kubri keeps the contexts you add in its own folder, readable only by you, and leaves your kubeconfig files as they are.
		helm and k9s plugins see them too.
	</p>
	<div
		class="drop"
		class:dragging
		role="region"
		aria-label="Drop a kubeconfig file here"
		ondragover={(e) => {
			e.preventDefault();
			dragging = true;
		}}
		ondragleave={() => (dragging = false)}
		ondrop={(e) => {
			e.preventDefault();
			dragging = false;
			read(e.dataTransfer?.files[0]);
		}}
	>
		<span>Drop a kubeconfig file here, or</span>
		<button type="button" class="btn" onclick={() => picker?.click()}>Choose file…</button>
		<input bind:this={picker} type="file" hidden onchange={(e) => read(e.currentTarget.files?.[0])} />
	</div>
	<label class="paste">
		<span>Or paste it{source ? ` (from ${source})` : ''}</span>
		<textarea
			bind:value={text}
			rows="5"
			spellcheck="false"
			placeholder="apiVersion: v1&#10;kind: Config&#10;clusters: …"
			onchange={() => {
				source = '';
				inspect();
			}}
		></textarea>
	</label>

	{#if found.length}
		<fieldset>
			<legend>Contexts in it</legend>
			{#each found as c (c.name)}
				<label class="context" class:off={!!c.problem}>
					<input type="checkbox" value={c.name} bind:group={chosen} disabled={!!c.problem} />
					<span>
						<strong>{c.name}</strong>
						<small class="mono">{c.server ?? 'no server'}{c.namespace ? ` · ${c.namespace}` : ''}{c.user ? ` · ${c.user}` : ''}</small>
						{#if c.problem}
							<small class="problem">{c.problem}</small>
						{:else if c.exec}
							<small class="exec">Signs in by running <code>{c.exec}</code>, which must be installed here.</small>
						{/if}
					</span>
				</label>
			{/each}
		</fieldset>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<div class="buttons">
		<button type="button" class="secondary" onclick={() => onclose([])} disabled={busy}>Cancel</button>
		<button type="button" class="primary" onclick={add} disabled={busy || !chosen.length}>
			{busy ? 'Adding…' : chosen.length > 1 ? `Add ${chosen.length} contexts` : 'Add context'}
		</button>
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(620px, calc(100vw - 32px));
		max-height: 88dvh;
		margin: auto;
		overflow: auto;
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
		margin: 0 0 6px;
		font-size: 17px;
	}
	.lead {
		margin: 0 0 14px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.drop {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 18px;
		border: 1.5px dashed var(--border);
		border-radius: 10px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.drop.dragging {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, var(--surface-1));
	}
	.paste {
		display: grid;
		gap: 5px;
		margin-top: 12px;
		font-size: 12px;
		color: var(--text-muted);
	}
	textarea {
		padding: 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		resize: vertical;
	}
	fieldset {
		display: grid;
		gap: 6px;
		margin: 14px 0 0;
		padding: 0;
		border: none;
	}
	legend {
		margin-bottom: 6px;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.context {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 10px;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		font-size: 13px;
	}
	.context.off {
		opacity: 0.75;
	}
	.context input {
		margin-top: 3px;
		accent-color: var(--accent);
	}
	.context span {
		display: grid;
		gap: 2px;
		min-width: 0;
	}
	small {
		color: var(--text-muted);
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	.mono,
	code {
		font-family: var(--mono);
	}
	.problem {
		color: var(--bad);
	}
	.exec {
		color: var(--warn);
	}
	.error {
		margin: 12px 0 0;
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
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
	.primary:disabled {
		opacity: 0.6;
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
</style>

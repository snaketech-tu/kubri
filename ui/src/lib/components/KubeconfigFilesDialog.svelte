<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { isTauri } from '@tauri-apps/api/core';
	import { untrack } from 'svelte';
	import { backend, type ContextList, type KubeconfigFile } from '../api';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';
	import AddContextsDialog from './AddContextsDialog.svelte';

	const desktop = isTauri();

	let box = $state<HTMLDialogElement>();
	let files = $state<KubeconfigFile[] | null>(null);
	let path = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	// Read once: the start page in the browser opens straight to copying contexts.
	const direct = untrack(() => app.kubeconfigFilesOpen === 'copy');
	let copying = $state(direct);

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		refresh();
		return () => previous?.focus();
	});

	function close() {
		if (!busy) app.kubeconfigFilesOpen = false;
	}

	async function refresh() {
		try {
			files = await backend.kubeconfigFiles();
		} catch (err) {
			error = errorMessage(err);
		}
	}

	/** Takes the new context list, and connects when Kubri had nothing to connect to yet. */
	async function done(list: ContextList) {
		app.contexts = list.contexts;
		await refresh();
		if (!app.cluster && !app.connecting && list.contexts.length) app.load();
	}

	async function load(paths: string[]) {
		if (!paths.length) return;
		busy = true;
		error = null;
		try {
			const before = new Set(app.contexts.map((c) => c.name));
			// From the start page: close, so the connection to the new contexts shows.
			const first = !before.size;
			const list = await backend.addKubeconfigFiles(paths);
			const added = list.contexts.filter((c) => !before.has(c.name)).length;
			path = '';
			await done(list);
			app.toast(
				`Loaded ${paths.length === 1 ? 'the file' : `${paths.length} files`}: ${added === 1 ? '1 new context' : `${added} new contexts`}`
			);
			if (first && list.contexts.length) app.kubeconfigFilesOpen = false;
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}

	async function choose() {
		error = null;
		try {
			await load((await backend.chooseKubeconfigFiles()) ?? []);
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function remove(file: KubeconfigFile) {
		busy = true;
		error = null;
		try {
			await done(await backend.removeKubeconfigFile(file.path));
			app.toast(`Kubri no longer reads ${file.path}`);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}

	function contextsText(file: KubeconfigFile): string {
		if (!file.contexts.length) return 'No contexts';
		const shown = file.contexts.slice(0, 6).join(', ');
		const more = file.contexts.length - 6;
		return more > 0 ? `${shown} and ${more} more` : shown;
	}
</script>

{#if !direct}
	<dialog
		{@attach lightDismiss}
		class="dialog"
		aria-label="Kubeconfig files"
		bind:this={box}
		oncancel={(e) => {
			e.preventDefault();
			close();
		}}
	>
		<h2>Kubeconfig files</h2>
		<p class="lead">
			{#if desktop}
				Kubri reads your kubeconfig as kubectl does. Load more files here: Kubri reads them where they are, so changes to them show up
				after a reload, and they stay loaded the next time Kubri starts. helm and k9s plugins see them too. Kubri never changes them.
			{:else}
				These are the files kubri-server reads. To use another cluster in the browser, add its contexts from a kubeconfig file.
			{/if}
		</p>

		{#if files === null && !error}
			<p class="muted">Reading…</p>
		{:else if files}
			<ul class="files">
				{#each files as file (file.path)}
					<li class:off={!!file.problem || file.missing}>
						<div class="row">
							<span class="path mono" title={file.path}>{file.path}</span>
							<span class="badge">{file.chosen ? 'Loaded in Kubri' : 'Your kubeconfig'}</span>
							{#if file.chosen && desktop}
								<button type="button" class="remove" onclick={() => remove(file)} disabled={busy} aria-label={`Stop reading ${file.path}`}>
									Remove
								</button>
							{/if}
						</div>
						{#if file.problem}
							<small class="problem">{file.problem}</small>
						{:else if file.missing}
							<small>Does not exist, so it is skipped</small>
						{:else}
							<small>{contextsText(file)}</small>
							{#if file.shadowed.length}
								<small class="warn">
									{file.shadowed.join(', ')}
									{file.shadowed.length === 1 ? 'is' : 'are'} also in an earlier file, which wins.
								</small>
							{/if}
						{/if}
					</li>
				{:else}
					<li class="muted">No kubeconfig files yet.</li>
				{/each}
			</ul>
		{/if}

		{#if desktop}
			<form
				class="load"
				onsubmit={(e) => {
					e.preventDefault();
					load([path]);
				}}
			>
				<input bind:value={path} placeholder="~/.kube/lab.yaml" aria-label="Path of a kubeconfig file" spellcheck="false" disabled={busy} />
				<button type="submit" class="secondary" disabled={busy || !path.trim()}>Load</button>
				<button type="button" class="primary" onclick={choose} disabled={busy}>Choose files…</button>
			</form>
		{/if}
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="buttons">
			{#if !desktop}
				<button type="button" class="primary" onclick={() => (copying = true)}>Add contexts from a file…</button>
			{/if}
			<button type="button" class="secondary" onclick={close} disabled={busy}>Close</button>
		</div>
	</dialog>
{/if}

{#if copying}
	<AddContextsDialog
		onclose={(names) => {
			copying = false;
			if (direct) app.kubeconfigFilesOpen = false;
			if (names.length) done({ contexts: app.contexts, cluster: app.cluster });
		}}
	/>
{/if}

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(640px, calc(100vw - 32px));
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
	.lead,
	.muted {
		margin: 0 0 14px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.files {
		display: grid;
		gap: 6px;
		margin: 0 0 14px;
		padding: 0;
		list-style: none;
	}
	.files li {
		display: grid;
		gap: 3px;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		font-size: 13px;
	}
	.files li.off {
		opacity: 0.8;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.path {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		flex: none;
		padding: 1px 7px;
		border-radius: 999px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 11px;
	}
	.remove {
		flex: none;
		padding: 3px 9px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.remove:hover:not(:disabled) {
		border-color: var(--bad);
		color: var(--bad);
	}
	small {
		color: var(--text-muted);
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12px;
	}
	.problem {
		color: var(--bad);
	}
	.warn {
		color: var(--warn);
	}
	.load {
		display: flex;
		gap: 8px;
	}
	.load input {
		flex: 1;
		min-width: 0;
		padding: 7px 9px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
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
	.buttons button,
	.load button {
		padding: 8px 14px;
		border-radius: 8px;
		white-space: nowrap;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled,
	.secondary:disabled {
		opacity: 0.6;
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	@media (max-width: 520px) {
		.load {
			flex-wrap: wrap;
		}
		.load input {
			flex-basis: 100%;
		}
	}
</style>

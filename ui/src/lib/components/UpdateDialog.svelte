<script lang="ts">
	import { backend, type UpdateCheck, type UpdateProgress } from '../api';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';

	let { release }: { release: UpdateCheck } = $props();

	let box = $state<HTMLDialogElement>();
	let primary = $state<HTMLButtonElement>();
	let progress = $state<UpdateProgress | null>(null);
	let error = $state<string | null>(null);
	const busy = $derived(progress !== null && error === null);

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		primary?.focus();
		return () => previous?.focus();
	});

	function later() {
		if (!busy) app.updateOpen = false;
	}

	async function install() {
		error = null;
		progress = { type: 'download', done: 0, total: null };
		try {
			// Kubri starts again once it is installed, so this only returns when it failed.
			await backend.installUpdate((p) => (progress = p));
		} catch (err) {
			error = errorMessage(err);
		}
	}

	const MB = 1024 * 1024;
	const megabytes = (bytes: number) => (bytes / MB).toFixed(bytes < 10 * MB ? 1 : 0);
	const percent = $derived(
		progress?.type === 'download' && progress.total ? Math.min(100, (progress.done / progress.total) * 100) : null
	);
	const status = $derived.by(() => {
		if (!progress) return '';
		if (progress.type === 'install') return 'Installing… Your system may ask for your password.';
		const { done, total } = progress;
		return total ? `Downloading… ${megabytes(done)} of ${megabytes(total)} MB` : `Downloading… ${megabytes(done)} MB`;
	});
</script>

<dialog
	class="dialog"
	aria-label="Update Kubri"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		later();
	}}
>
	<h2>Kubri {release.version} is out</h2>
	<p>
		You have {release.current}. Kubri can download the update, check its signature, install it and start again. Port-forwards,
		shells and log streams close when it does.
	</p>
	<p class="muted">
		<button class="link" onclick={() => backend.openUrl(release.url)}>What's new in {release.version}</button>
	</p>

	{#if progress && !error}
		<div class="progress" role="status" aria-live="polite">
			<div
				class="bar"
				class:indeterminate={percent === null}
				role="progressbar"
				aria-label="Update progress"
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={percent === null ? undefined : Math.round(percent)}
			>
				<span style:width={percent === null ? null : `${percent}%`}></span>
			</div>
			<span class="muted small">{status}</span>
		</div>
	{/if}
	{#if error}
		<p class="error" role="alert">{error}</p>
		<p class="muted small">You can download it from the release page and install it by hand instead.</p>
	{/if}

	<div class="buttons">
		{#if error}
			<button class="secondary" onclick={later}>Close</button>
			<button class="secondary" onclick={() => backend.openUrl(release.url)}>Open the download page</button>
			<button class="primary" onclick={install}>Try again</button>
		{:else}
			<button class="secondary" onclick={() => app.dismissRelease()} disabled={busy}>Skip this version</button>
			<button class="secondary" onclick={later} disabled={busy}>Later</button>
			<button class="primary" onclick={install} disabled={busy} bind:this={primary}>{busy ? 'Updating…' : 'Update and restart'}</button>
		{/if}
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(480px, calc(100vw - 32px));
		max-height: 85dvh;
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
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	.error {
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}
	.progress {
		display: grid;
		gap: 6px;
		margin: 4px 0 14px;
	}
	.bar {
		position: relative;
		height: 6px;
		overflow: hidden;
		border-radius: 999px;
		background: var(--surface-2);
	}
	.bar span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--accent);
		transition: width 0.2s ease;
	}
	.bar.indeterminate span {
		width: 30%;
		animation: slide 1.2s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(340%);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.bar.indeterminate span {
			width: 100%;
			animation: none;
			opacity: 0.5;
		}
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
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
	button:disabled {
		opacity: 0.6;
		cursor: default;
	}
</style>

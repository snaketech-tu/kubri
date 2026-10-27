<script lang="ts">
	import { app } from '../state.svelte';
</script>

<div class="toasts" aria-live="polite">
	{#each app.toasts as toast (toast.id)}
		<div class="toast {toast.tone}">{toast.text}</div>
	{/each}
</div>

<style>
	.toasts {
		position: fixed;
		right: 16px;
		bottom: 40px;
		z-index: 60;
		display: flex;
		flex-direction: column;
		gap: 8px;
		max-width: min(420px, calc(100vw - 32px));
	}
	.toast {
		padding: 10px 14px;
		border-radius: 8px;
		border-left: 4px solid var(--ok);
		background: var(--surface-1);
		box-shadow: var(--shadow);
		font-size: 13px;
	}
	.toast.bad {
		border-left-color: var(--bad);
	}
</style>

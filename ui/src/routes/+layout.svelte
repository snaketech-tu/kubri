<script lang="ts">
	import '../app.css';
	import { isTauri } from '@tauri-apps/api/core';
	import logo from '#lib/assets/logo.svg';
	import DraftDialog from '#lib/components/DraftDialog.svelte';
	import SignIn from '#lib/components/SignIn.svelte';
	import { drafts } from '#lib/navigation.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	$effect(() => {
		session.check();
	});
	// Closing the desktop window with an unsaved YAML draft asks first; the guard closes it on the user's say.
	$effect(() => {
		if (!isTauri()) return;
		let disposed = false;
		let unlisten: (() => void) | undefined;
		import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
			const win = getCurrentWindow();
			const stop = await win.onCloseRequested((event) => {
				if (drafts.block(() => win.destroy())) event.preventDefault();
			});
			if (disposed) stop();
			else unlisten = stop;
		});
		return () => {
			disposed = true;
			unlisten?.();
		};
	});

	// The browser can only ask its own question before leaving; it cannot wait for a save.
	function onbeforeunload(e: BeforeUnloadEvent) {
		if (!drafts.dirty && !drafts.saving) return;
		e.preventDefault();
		e.returnValue = '';
	}
</script>

<svelte:window {onbeforeunload} />

<svelte:head>
	<title>Kubri</title>
	<link rel="icon" href={logo} />
</svelte:head>

<DraftDialog />

<!-- The app starts once signed in, and stops (closing its sockets) when the session ends. -->
{#if session.checked}
	{#if session.signedIn}
		{@render children()}
	{:else}
		<SignIn />
	{/if}
{/if}

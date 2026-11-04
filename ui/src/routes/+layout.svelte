<script lang="ts">
	import '../app.css';
	import { isTauri } from '@tauri-apps/api/core';
	import logo from '#lib/assets/logo.svg';
	import DraftDialog from '#lib/components/DraftDialog.svelte';
	import SignIn from '#lib/components/SignIn.svelte';
	import { backend } from '#lib/api/index.js';
	import { errorMessage } from '#lib/format.js';
	import { externalHref } from '#lib/links.js';
	import { app } from '#lib/state.svelte.js';
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

	// The desktop window opens no new windows, and leaving the app would lose it: every link to
	// another site, like the install guides on the Integrations page, opens in the browser.
	function onclick(e: MouseEvent) {
		if (!backend.desktop || e.defaultPrevented || e.button > 1) return;
		const link = e.target instanceof Element ? e.target.closest('a[href]') : null;
		const href = externalHref(link?.getAttribute('href'), location.origin);
		if (!href) return;
		e.preventDefault();
		backend.openUrl(href).catch((err) => app.toast(errorMessage(err), 'bad'));
	}

	// The browser can only ask its own question before leaving; it cannot wait for a save.
	function onbeforeunload(e: BeforeUnloadEvent) {
		if (!drafts.dirty && !drafts.saving) return;
		e.preventDefault();
		e.returnValue = '';
	}
</script>

<svelte:window {onbeforeunload} />
<svelte:document {onclick} onauxclick={onclick} />

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

<script lang="ts">
	import '../app.css';
	import logo from '#lib/assets/logo.svg';
	import SignIn from '#lib/components/SignIn.svelte';
	import { session } from '#lib/session.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	$effect(() => {
		session.check();
	});
</script>

<svelte:head>
	<title>Kubri</title>
	<link rel="icon" href={logo} />
</svelte:head>

<!-- The app starts once signed in, and stops (closing its sockets) when the session ends. -->
{#if session.checked}
	{#if session.signedIn}
		{@render children()}
	{:else}
		<SignIn />
	{/if}
{/if}

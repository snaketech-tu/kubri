<script lang="ts">
	import '../app.css';
	import logo from '#lib/assets/logo.svg';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<title>Kubri</title>
	<link rel="icon" href={logo} />
</svelte:head>

{@render children()}

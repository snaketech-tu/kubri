<script lang="ts">
	import { untrack } from 'svelte';
	import { backend } from '../api';
	import type { DockSession } from '../dock.svelte';
	import { containerNames } from '../pods';
	import LocalTerminal from './LocalTerminal.svelte';
	import LogView from './LogView.svelte';
	import TerminalView from './TerminalView.svelte';

	let { session, active }: { session: DockSession; active: boolean } = $props();
	const s = untrack(() => session);

	/** The pod's containers, for the picker; a workload's logs need none. */
	let containers = $state<string[]>([]);
	$effect(() => {
		if (s.kind === 'local') return;
		backend.getObject(s.target).then(
			(object) => (containers = containerNames(object)),
			() => {}
		);
	});
	const picked = s.kind !== 'local' && s.container ? { name: s.container, previous: s.previous } : null;
</script>

{#if s.kind === 'local'}
	<LocalTerminal session={s} />
{:else if s.kind === 'logs'}
	<LogView
		target={s.target}
		{containers}
		previous={s.previous}
		container={picked}
		{active}
	/>
{:else}
	<TerminalView target={s.target} {containers} mode={s.mode} container={s.container ? { name: s.container, mode: s.mode } : null} />
{/if}

<script lang="ts">
	import { parts } from '../i18n';

	/**
	 * A translated text with its `{name}`s filled in: from `strong` in bold, from `code` as code and
	 * from `mono` in a monospaced font. A name in none of them stays as it is.
	 */
	let {
		text,
		strong = {},
		code = {},
		mono = {}
	}: {
		text: string;
		strong?: Record<string, string | number>;
		code?: Record<string, string | number>;
		mono?: Record<string, string | number>;
	} = $props();
</script>

{#each parts(text) as part, i (i)}{#if 'text' in part}{part.text}{:else if Object.hasOwn(strong, part.name)}<strong>{strong[part.name]}</strong
		>{:else if Object.hasOwn(code, part.name)}<code>{code[part.name]}</code>{:else if Object.hasOwn(mono, part.name)}<span class="mono"
			>{mono[part.name]}</span
		>{:else}{`{${part.name}}`}{/if}{/each}

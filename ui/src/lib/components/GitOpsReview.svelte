<script lang="ts">
	import { targetName, type AutoSync } from '../action-dialog';
	import type { GitOpsAction, ObjectRef } from '../api';
	import { t } from '../i18n';
	import Phrase from './Phrase.svelte';

	let { target, action, autoSync }: { target: ObjectRef; action: GitOpsAction | 'autoSync'; autoSync: AutoSync | null } = $props();
	const name = $derived(targetName(target));
</script>

{#if action === 'suspend'}
	<p>
		<Phrase
			text={t(
				'Suspend {name}? Flux stops reconciling it, so changes in its source are not applied and changes by hand are not put back, until it is resumed.'
			)}
			strong={{ name }}
		/>
	</p>
{:else if action === 'resume'}
	<p>
		<Phrase text={t('Resume {name}? Flux reconciles it now and on its interval again, putting back changes made by hand.')} strong={{ name }} />
	</p>
{:else if action === 'sync'}
	<p>
		<Phrase
			text={t('Sync {name} to its target revision? Argo CD applies what Git says, running its sync hooks, as argocd app sync does.')}
			strong={{ name }}
		/>
	</p>
{:else if action === 'autoSync'}
	{#if !autoSync}
		<p class="muted">{t('Reading the Application…')}</p>
	{:else if autoSync.on}
		<p>
			<Phrase
				text={autoSync.options.length
					? t(
							'Auto-sync of {name} is on ({options}). Turn it off? Argo CD then applies changes from Git only when someone syncs, and leaves changes by hand alone. Kubri keeps the settings to turn it on again as it was.',
							{ options: autoSync.options.map((o) => (o === 'selfHeal' ? 'self-heal' : o)).join(', ') }
						)
					: t(
							'Auto-sync of {name} is on. Turn it off? Argo CD then applies changes from Git only when someone syncs, and leaves changes by hand alone. Kubri keeps the settings to turn it on again as it was.'
						)}
				strong={{ name }}
			/>
		</p>
	{:else}
		<p>
			<Phrase text={t('Turn auto-sync of {name} on? Argo CD then applies changes from Git by itself.')} strong={{ name }} />
		</p>
	{/if}
{/if}

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
</style>

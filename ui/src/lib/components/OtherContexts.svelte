<script lang="ts">
	import { t } from '../i18n';
	import { untrack } from 'svelte';
	import { relative } from '../format';
	import { CHECK_AT_MOST, reach, STALE_MS, type ContextReach } from '../reach.svelte';
	import { app } from '../state.svelte';

	/** Contexts shown here; the Clusters page has them all. */
	const SHOWN = 8;

	const others = $derived(app.contexts.filter((c) => c.name !== app.cluster?.context));

	// Checks the other contexts once when the overview opens, unless that was done lately.
	$effect(() => {
		const names = untrack(() => others.map((c) => c.name));
		if (names.length && names.length <= CHECK_AT_MOST) void reach.refresh(names, STALE_MS);
	});

	/** The dot: filled in the health color, a ring when the API server did not answer. */
	function tone(known: ContextReach | undefined) {
		if (!known) return 'unknown';
		if (!known.reachable) return 'down';
		return known.health ?? 'unknown';
	}
</script>

{#if others.length && !app.cluster?.snapshot}
	<section class="others" aria-labelledby="others-title">
		<div class="head">
			<h2 id="others-title">{t('Other contexts')}</h2>
			<button class="link" onclick={() => app.showFleet()}>{t('All clusters')}</button>
		</div>
		<ul>
			{#each others.slice(0, SHOWN) as ctx (ctx.name)}
				{@const known = reach.of[ctx.name]}
				{@const settings = app.prefs.contextSettingsOf(ctx.name)}
				<li>
					<button class="context" onclick={() => app.switchContext(ctx.name)} title={known?.error ? t('Switch to {name}. Last error: {error}', { name: ctx.name, error: known.error }) : t('Switch to {name}', { name: ctx.name })}>
						<span class="dot {tone(known)}" aria-hidden="true"></span>
						<span class="text">
							<span class="name">
								{#if settings.color}<span class="swatch" style:background={settings.color}></span>{/if}
								{app.prefs.contextName(ctx.name)}
								{#if settings.production}<span class="badge">{t('production')}</span>{/if}
							</span>
							<span class="state">{known ? `${known.summary}, ${relative(new Date(known.at).toISOString(), Date.now())}` : t('Not checked yet')}</span>
						</span>
					</button>
				</li>
			{/each}
			{#if others.length > SHOWN}
				<li>
					<button class="context more" onclick={() => app.showFleet()}>{t('{n} more on the Clusters page', { n: others.length - SHOWN })}</button>
				</li>
			{/if}
		</ul>
	</section>
{/if}

<style>
	.others {
		margin-top: 20px;
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 10px;
	}
	h2 {
		margin: 0;
		font-size: 15px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 13px;
		font-weight: 600;
	}
	ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 230px), 1fr));
		gap: 10px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.context {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		width: 100%;
		height: 100%;
		padding: 12px 14px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		text-align: left;
	}
	.context:hover {
		border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
		background: var(--surface-2);
	}
	.more {
		align-items: center;
		justify-content: center;
		color: var(--accent-strong);
		font-size: 13px;
		font-weight: 600;
	}
	.dot {
		flex: none;
		width: 9px;
		height: 9px;
		margin-top: 5px;
		border-radius: 50%;
		background: var(--text-muted);
		opacity: 0.6;
	}
	.dot.ok {
		background: var(--ok);
		opacity: 1;
	}
	.dot.warning {
		background: var(--warn);
		opacity: 1;
	}
	.dot.failed {
		background: var(--bad);
		opacity: 1;
	}
	.dot.down {
		border: 2px solid var(--bad);
		background: none;
		opacity: 1;
	}
	.text {
		display: grid;
		gap: 2px;
		min-width: 0;
	}
	.name {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		color: var(--text);
		font-size: 13.5px;
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.swatch {
		width: 10px;
		height: 10px;
		border-radius: 3px;
	}
	.badge {
		padding: 0 6px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
		font-size: 10.5px;
		font-weight: 600;
	}
	.state {
		color: var(--text-muted);
		font-size: 12px;
	}
</style>

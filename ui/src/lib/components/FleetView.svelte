<script lang="ts">
	import { backend, type FleetCluster, type KindPulse } from '../api';
	import { connectError, eachLimited, fleetState, serverHost, workloads } from '../fleet';
	import { cpu, errorMessage, memory, relative } from '../format';
	import { meterTone } from '../pulse';
	import { app } from '../state.svelte';
	import AddContextsDialog from './AddContextsDialog.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';

	/** Checked again while the page is open. */
	const REFRESH_MS = 60_000;
	/** Contexts checked at once; each gives up after ten seconds on its own. */
	const AT_ONCE = 6;

	let found = $state<Record<string, FleetCluster>>({});
	/** Why a context could not be asked at all, such as a broken kubeconfig. */
	let failed = $state<Record<string, string>>({});
	let checking = $state(false);
	let checked = $state(0);
	let now = $state(Date.now());
	let adding = $state(false);
	/** The added context whose Remove was pressed once, asking to press again. */
	let removing = $state<string | null>(null);

	const contexts = $derived(app.contexts.map((c) => c.name));
	const states = $derived(Object.fromEntries(contexts.map((name) => [name, fleetState(found[name] ?? null)])));
	const tally = $derived.by(() => {
		const all = Object.values(states);
		return {
			ok: all.filter((s) => s.health === 'ok').length,
			warning: all.filter((s) => s.health === 'warning').length,
			failed: all.filter((s) => s.health === 'failed' && !s.unreachable).length,
			unreachable: all.filter((s) => s.unreachable).length
		};
	});

	async function check() {
		if (checking) return;
		checking = true;
		try {
			await eachLimited([...contexts], AT_ONCE, async (name) => {
				try {
					found[name] = await backend.fleetCluster(name);
					delete failed[name];
				} catch (err) {
					failed[name] = errorMessage(err);
				}
			});
			checked = now = Date.now();
		} finally {
			checking = false;
		}
	}

	$effect(() => {
		if (!contexts.length) return;
		check();
		const timer = setInterval(check, REFRESH_MS);
		const clock = setInterval(() => (now = Date.now()), 10_000);
		return () => {
			clearInterval(timer);
			clearInterval(clock);
		};
	});

	async function open(name: string) {
		if (app.cluster?.context !== name) await app.switchContext(name);
		if (app.cluster?.context === name) app.showPulses();
	}

	async function remove(name: string) {
		if (removing !== name) {
			removing = name;
			return;
		}
		removing = null;
		try {
			app.contexts = (await backend.removeContext(name)).contexts;
			delete found[name];
			app.toast(`Removed ${name} from Kubri`);
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
	}

	const percent = (part: number, whole: number) => (whole > 0 ? part / whole : 0);
	const counted = (k: KindPulse | null | undefined) => (k && !k.error ? `${k.ok}/${k.total}` : '—');
</script>

<section class="page">
	<PageHeader
		crumbs={['Clusters']} prefix="none"
		title="Clusters"
		count={contexts.length}
		subtitle="Every context in your kubeconfig, checked side by side without connecting to it"
	>
		{#snippet tools()}
			{#if checked}<span class="muted small">Checked {relative(new Date(checked).toISOString(), now)}</span>{/if}
			<button class="btn" onclick={check} disabled={checking}>{checking ? 'Checking…' : 'Check again'}</button>
			<button class="btn" onclick={() => (adding = true)}>Add clusters…</button>
		{/snippet}
	</PageHeader>

	{#if !contexts.length}
		<p class="muted">No contexts in your kubeconfig.</p>
	{:else}
		<p class="tally" role="status">
			{#if tally.ok}<span><HealthIcon health="ok" /> {tally.ok} healthy</span>{/if}
			{#if tally.warning}<span><HealthIcon health="warning" /> {tally.warning} need a look</span>{/if}
			{#if tally.failed}<span><HealthIcon health="failed" /> {tally.failed} with failing workloads</span>{/if}
			{#if tally.unreachable}<span><HealthIcon health="failed" /> {tally.unreachable} unreachable</span>{/if}
		</p>
		<div class="grid">
			{#each contexts as name (name)}
				{@const settings = app.contextSettingsOf(name)}
				{@const c = found[name]}
				{@const state = states[name]}
				{@const nodes = c?.kinds.find((k) => k.kind === 'Node')}
				{@const pods = c?.kinds.find((k) => k.kind === 'Pod')}
				{@const work = c ? workloads(c.kinds) : null}
				{@const added = app.contexts.find((x) => x.name === name)?.added}
				<article
					class="cluster {state.health ?? 'checking'}"
					class:current={app.cluster?.context === name}
					style:--context-color={settings.color || null}
				>
					<span class="head">
						<span class="swatch" aria-hidden="true"></span>
						<span class="names">
							<button
								class="name open"
								onclick={() => open(name)}
								disabled={!!app.connecting}
								title={app.cluster?.context === name ? 'Show its overview' : `Connect to ${name}`}
							>
								{app.contextName(name)}
							</button>
							{#if settings.displayName}<span class="context mono">{name}</span>{/if}
						</span>
						{#if settings.production}<span class="tag prod">Production</span>{:else if settings.label}<span class="tag">{settings.label}</span>{/if}
						{#if app.cluster?.context === name}<span class="tag here">Connected</span>{/if}
					</span>
					<span class="state">
						{#if state.health}<HealthIcon health={state.health} size={11} />{:else}<Icon name="pending" size={13} />{/if}
						<span>{failed[name] ? 'Could not be checked' : state.summary}</span>
					</span>
					{#if failed[name] || (c?.error && !c.version)}
						<span class="error" title={failed[name] ?? c?.error ?? undefined}>{failed[name] ?? connectError(c?.error ?? '')}</span>
					{:else if c?.version}
						<span class="counts">
							<span><small>Nodes</small>{counted(nodes)}</span>
							<span><small>Pods</small>{counted(pods)}</span>
							<span><small>Workloads</small>{counted(work)}</span>
						</span>
						{#if c.cpu && c.memory}
							{@const usage = [
								['CPU requested', c.cpu, (v: number) => cpu(v)],
								['Memory requested', c.memory, (v: number) => memory(v)]
							] as const}
							<span class="meters">
								{#each usage as [label, [requested, capacity], format] (label)}
									{@const share = percent(requested, capacity)}
									<span class="meter-row" title="{format(requested)} requested of {format(capacity)}">
										<small>{label}</small>
										<span class="meter" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(share * 100)} aria-label={label}>
											<span class="fill {meterTone(share)}" style:width="{Math.min(share, 1) * 100}%"></span>
										</span>
										<span class="mono">{Math.round(share * 100)}%</span>
									</span>
								{/each}
							</span>
						{/if}
						{#if c.error}<span class="note">{c.error}</span>{/if}
						<span class="meta mono">{serverHost(c.server)} · {c.version} · {c.millis} ms</span>
					{/if}
					{#if added}
						<span class="added">
							<small>Added in Kubri</small>
							<button class="btn small remove" class:danger={removing === name} onclick={() => remove(name)} onblur={() => removing === name && (removing = null)}>
								{removing === name ? 'Remove from Kubri?' : 'Remove'}
							</button>
						</span>
					{/if}
				</article>
			{/each}
		</div>
		<p class="muted small">Meters show what pods request of what nodes can hold. Opening a cluster connects Kubri to it.</p>
	{/if}
</section>

{#if adding}
	<AddContextsDialog
		onclose={(names) => {
			adding = false;
			if (names.length) check();
		}}
	/>
{/if}

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
		overflow: auto;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	.mono {
		font-family: var(--mono);
	}
	.tally {
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
		margin: 0 0 12px;
		font-size: 13px;
		min-height: 18px;
	}
	.tally span {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
		gap: 14px;
	}
	.cluster {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px 12px;
		border: 1px solid var(--border);
		border-top: 3px solid var(--context-color, var(--border));
		border-radius: 10px;
		background: var(--surface-1);
		color: var(--text);
		text-align: left;
	}
	.cluster:hover {
		border-color: var(--text-muted);
		border-top-color: var(--context-color, var(--text-muted));
	}
	.cluster.failed {
		background: color-mix(in srgb, var(--bad) 5%, var(--surface-1));
		border-color: color-mix(in srgb, var(--bad) 40%, var(--border));
		border-top-color: var(--context-color, var(--bad));
	}
	.cluster.warning {
		border-color: color-mix(in srgb, var(--warn) 40%, var(--border));
		border-top-color: var(--context-color, var(--warn));
	}
	.cluster.checking {
		opacity: 0.75;
	}
	.cluster.current {
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.head {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.swatch {
		flex: none;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--context-color, var(--border));
	}
	.names {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}
	.name {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		text-align: left;
		font-size: 15px;
		font-weight: 650;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.context {
		font-size: 11.5px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tag {
		flex: none;
		padding: 2px 7px;
		border-radius: 999px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 650;
	}
	.tag.prod {
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.tag.here {
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-1));
		color: var(--accent-strong);
	}
	.state {
		display: flex;
		align-items: baseline;
		gap: 7px;
		font-size: 13px;
		font-weight: 600;
	}
	.error,
	.note {
		font-size: 12px;
		color: var(--bad);
		overflow-wrap: anywhere;
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.note {
		color: var(--warn);
	}
	.counts {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 8px;
		font-size: 16px;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
	}
	.counts span,
	.meter-row {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	small {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.meters {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}
	.meter-row {
		display: grid;
		grid-template-columns: 112px 1fr 38px;
		align-items: center;
		gap: 8px;
		font-size: 12px;
	}
	.meter-row .mono {
		text-align: right;
	}
	.meter {
		position: relative;
		height: 5px;
		border-radius: 3px;
		background: var(--surface-2);
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 3px;
		background: var(--accent);
	}
	.fill.warn {
		background: var(--warn-mark);
	}
	.fill.bad {
		background: var(--bad);
	}
	/* The name's button covers the card, so the whole card opens the cluster. */
	.open::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: 10px;
	}
	.open:focus-visible {
		outline: none;
	}
	.open:focus-visible::after {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
	.cluster:has(.open:disabled) {
		cursor: progress;
	}
	.cluster:has(.open:not(:disabled)) {
		cursor: pointer;
	}
	.added {
		position: relative;
		z-index: 1;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding-top: 8px;
		border-top: 1px solid var(--border-subtle);
	}
	.meta {
		font-size: 11.5px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.page > .muted.small {
		margin-top: 14px;
	}
</style>

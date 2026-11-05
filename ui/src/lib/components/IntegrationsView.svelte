<script lang="ts">
	import type { Integration, IntegrationSettings } from '../api';
	import { openTool } from '../integrations';
	import { app } from '../state.svelte';
	import DashboardList from './DashboardList.svelte';
	import PageHeader from './PageHeader.svelte';

	/** What each tool adds, and the kinds to open for the operators. */
	const ABOUT: Record<string, { text: string; install?: string; kinds?: { group: string; plural: string; label: string }[] }> = {
		prometheus: { install: 'https://github.com/prometheus-community/helm-charts/tree/main/charts/kube-prometheus-stack', text: 'Charts of CPU, memory, restarts, network and requests for pods, workloads and nodes (m), and an hour of history in the overview.' },
		grafana: { install: 'https://grafana.com/docs/grafana/latest/setup-grafana/installation/kubernetes/', text: 'The logs of the namespace, pod, workload or Service you pick in Grafana’s Explore, and the dashboards that show it.' },
		loki: { install: 'https://grafana.com/docs/loki/latest/setup/install/helm/', text: 'Log history beyond a pod’s lifetime, in the log workspace.' },
		flux: {
			install: 'https://fluxcd.io/flux/installation/',
			text: 'Kustomizations, HelmReleases and their sources.',
			kinds: [
				{ group: 'kustomize.toolkit.fluxcd.io', plural: 'kustomizations', label: 'Kustomizations' },
				{ group: 'helm.toolkit.fluxcd.io', plural: 'helmreleases', label: 'HelmReleases' },
				{ group: 'source.toolkit.fluxcd.io', plural: 'gitrepositories', label: 'Git repositories' }
			]
		},
		argocd: {
			install: 'https://argo-cd.readthedocs.io/en/stable/getting_started/',
			text: 'Applications, their sync state and health.',
			kinds: [{ group: 'argoproj.io', plural: 'applications', label: 'Applications' }]
		},
		certManager: {
			install: 'https://cert-manager.io/docs/installation/',
			text: 'Certificates, when they expire, and their issuers.',
			kinds: [
				{ group: 'cert-manager.io', plural: 'certificates', label: 'Certificates' },
				{ group: 'cert-manager.io', plural: 'issuers', label: 'Issuers' },
				{ group: 'cert-manager.io', plural: 'clusterissuers', label: 'Cluster issuers' }
			]
		}
	};
	const URLS: Partial<Record<string, { key: 'prometheus' | 'grafana' | 'loki'; example: string }>> = {
		prometheus: { key: 'prometheus', example: 'http://prometheus.monitoring.svc:9090' },
		grafana: { key: 'grafana', example: 'https://grafana.example.com' },
		loki: { key: 'loki', example: 'http://loki-gateway.logging.svc' }
	};

	let checking = $state(false);
	let saving = $state(false);
	const context = $derived(app.cluster?.context ?? '');
	const EMPTY: IntegrationSettings = { prometheus: '', grafana: '', grafanaToken: '', loki: '', grafanaLogs: '' };
	let draft = $state<IntegrationSettings>({ ...EMPTY });
	const saved = $derived({ ...EMPTY, ...app.contextSettings.integrations });
	const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(saved));

	$effect(() => {
		draft = { ...saved };
	});

	async function check() {
		checking = true;
		await app.loadIntegrations(true);
		checking = false;
	}
	$effect(() => {
		void context;
		check();
	});

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		const ok = await app.setContextSettings(context, { ...app.contextSettings, integrations: { ...draft } });
		saving = false;
		if (ok) {
			app.toast('Saved; looking again');
			await check();
		}
	}

	function status(item: Integration): { text: string; tone: 'ok' | 'bad' | 'muted' } {
		if (item.source === 'off') return { text: 'Off', tone: 'muted' };
		if (item.source === 'missing') return { text: 'Not found', tone: 'muted' };
		if (item.error) return { text: 'Not answering', tone: 'bad' };
		const how = item.source === 'settings' ? 'From settings' : 'Found';
		return { text: item.version ? `${how} · ${item.version}` : how, tone: 'ok' };
	}

	/** Tools the cluster runs, or that the settings point to, as cards; the rest as a short list below. */
	const present = $derived(app.integrations?.items.filter((i) => i.source !== 'missing' && i.source !== 'off') ?? []);
	const absent = $derived(app.integrations?.items.filter((i) => i.source === 'missing' || i.source === 'off') ?? []);

	function where(item: Integration): string | null {
		const e = item.endpoint;
		if (!e) return null;
		return e.via === 'url' ? e.url : `Service ${e.namespace}/${e.name}:${e.port}, through the API server`;
	}

	/** The namespace picked in the sidebar, whose dashboards the Grafana card lists. */
	const namespaceRef = $derived(
		app.namespace ? { resource: { group: '', version: 'v1', plural: 'namespaces' }, namespace: null, name: app.namespace } : null
	);

	function openKind(group: string, plural: string) {
		const resource = app.resources.find((r) => r.group === group && r.plural === plural);
		if (resource) app.open(resource);
	}
</script>

<section class="page">
	<PageHeader crumbs={['Integrations']} prefix="context" title="Integrations" subtitle="Tools the cluster runs, found without setup">
		{#snippet tools()}
			<button class="btn" onclick={check} disabled={checking}>{checking ? 'Looking…' : 'Look again'}</button>
		{/snippet}
	</PageHeader>

	<div class="body">
		{#if !app.integrations}
			<p class="muted">{checking ? 'Looking for Prometheus, Grafana, Loki, Flux, Argo CD and cert-manager…' : 'Kubri could not look for integrations.'}</p>
		{:else}
			{#if present.length}
				<ul class="tools">
					{#each present as item (item.id)}
						{@const s = status(item)}
						{@const about = ABOUT[item.id]}
						<li>
							<div class="head">
								<strong>{item.name}</strong>
								<span class="status {s.tone}">{s.text}</span>
							</div>
							<p class="about">{about?.text}</p>
							{#if where(item)}<p class="where">{where(item)}</p>{/if}
							{#if item.error}<p class="error">{item.error}</p>{/if}
							{#if item.id === 'grafana' && item.endpoint && !item.error}
								<div class="kinds">
									<button class="btn" onclick={() => openTool('grafana', '/')}>Open Grafana</button>
								</div>
								{#if namespaceRef}
									<div class="dashboards">
										<DashboardList target={namespaceRef} title="Grafana for {app.namespace}" empty="Grafana has nothing for {app.namespace}." />
									</div>
								{/if}
							{/if}
							{#if about?.kinds && item.source === 'found'}
								<div class="kinds">
									{#each about.kinds.filter((k) => app.resources.some((r) => r.group === k.group && r.plural === k.plural)) as kind (kind.plural)}
										<button class="btn" onclick={() => openKind(kind.group, kind.plural)}>{kind.label}</button>
									{/each}
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
			{#if absent.length}
				<div class="absent">
					<h2>{present.length ? 'Not in this cluster' : 'None of these runs in this cluster'}</h2>
					<ul>
						{#each absent as item (item.id)}
							{@const about = ABOUT[item.id]}
							<li>
								<strong>{item.name}</strong>
								<span class="muted">{about?.text}</span>
								{#if item.source === 'off'}
									<span class="muted off">Off</span>
								{:else if about?.install}
									<a href={about.install} target="_blank" rel="noreferrer" aria-label="How to install {item.name}">How to install</a>
								{/if}
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}

		<form class="settings" onsubmit={save}>
			<h2>Point elsewhere</h2>
			<p class="muted">For {app.contextName(context)} only. Leave a field empty and Kubri finds the tool in the cluster.</p>
			<details class="help">
				<summary>How to fill these in</summary>
				<p>
					Type <code>off</code> to turn a tool off. A URL like <code>http://name.namespace.svc:port</code> reaches that Service through the
					API server.
				</p>
				<p>
					<strong>Grafana URL</strong> is the address only, like <code>https://grafana.example.com</code>. <strong>Logs in Grafana</strong>
					on a namespace, pod, workload or Service then opens its logs in Grafana’s Explore, and <strong>Grafana…</strong> lists the dashboards
					that show it, ready to open or copy. Both work also when Grafana needs a login or a client certificate, as your browser opens them.
				</p>
				<p>
					<strong>Loki data source</strong> is the name or UID of the Loki data source in Grafana. Leave it empty unless Kubri picks the wrong
					one.
				</p>
				<p>
					<strong>Token</strong> lets Kubri list Grafana’s dashboards and find its Loki data source itself. It goes only to a Grafana given by
					URL.
				</p>
			</details>
			{#each [URLS.prometheus, URLS.loki] as url (url!.key)}
				<label>
					<span>{url!.key === 'prometheus' ? 'Prometheus' : 'Loki'} URL</span>
					<input bind:value={draft[url!.key]} placeholder="e.g. {url!.example}" spellcheck="false" autocomplete="off" />
				</label>
			{/each}
			<fieldset class="grafana">
				<legend>Grafana</legend>
				<label>
					<span>URL</span>
					<input bind:value={draft.grafana} placeholder="e.g. {URLS.grafana!.example}" spellcheck="false" autocomplete="off" />
				</label>
				<label>
					<span>Loki data source</span>
					<input bind:value={draft.grafanaLogs} placeholder="Found in Grafana, else its default" spellcheck="false" autocomplete="off" />
				</label>
				<label>
					<span>Token</span>
					<input type="password" bind:value={draft.grafanaToken} placeholder="A service account token with the Viewer role" autocomplete="off" />
				</label>
				<p class="muted small">Kept in Kubri’s settings file.</p>
			</fieldset>
			<div class="actions">
				<button class="btn primary" type="submit" disabled={!dirty || saving || !context}>{saving ? 'Saving…' : 'Save'}</button>
			</div>
		</form>
	</div>
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		height: 100%;
		padding: 0 24px;
	}
	.body {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(280px, 380px);
		gap: 24px;
		flex: 1;
		min-height: 0;
		padding-bottom: 24px;
		overflow: auto;
		align-items: start;
	}
	@media (max-width: 900px) {
		.body {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.tools {
		display: grid;
		gap: 12px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.tools li,
	.settings {
		padding: 14px 16px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
	}
	.status {
		font-size: 12.5px;
	}
	.status.ok {
		color: var(--ok);
	}
	.status.bad {
		color: var(--bad);
	}
	.status.muted,
	.muted,
	.where {
		color: var(--text-muted);
	}
	p {
		margin: 6px 0 0;
		font-size: 12.5px;
	}
	.where {
		font-family: var(--mono);
		font-size: 12px;
	}
	.error {
		color: var(--bad);
	}
	.dashboards {
		margin-top: 10px;
	}
	.kinds {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 10px;
	}
	.settings {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.settings h2 {
		margin: 0;
		font-size: 14px;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12.5px;
	}
	input {
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-family: var(--mono);
		font-size: 12px;
	}
	/* Examples, not values: in the text font and lighter, so an empty field does not look filled in. */
	input::placeholder {
		color: color-mix(in srgb, var(--text-muted) 75%, transparent);
		font-family: Inter, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
		font-style: italic;
	}
	.absent {
		margin-top: 16px;
	}
	.tools + .absent {
		margin-top: 20px;
	}
	.absent:first-child {
		margin-top: 0;
	}
	.absent h2 {
		margin: 0 0 6px;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.absent ul {
		margin: 0;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		list-style: none;
	}
	.absent li {
		display: grid;
		grid-template-columns: 120px minmax(0, 1fr) auto;
		align-items: baseline;
		gap: 4px 14px;
		padding: 9px 14px;
		font-size: 12.5px;
	}
	.absent li + li {
		border-top: 1px solid var(--border-subtle);
	}
	.absent a {
		white-space: nowrap;
	}
	@media (max-width: 640px) {
		.absent li {
			grid-template-columns: minmax(0, 1fr) auto;
		}
		.absent li .muted:not(.off) {
			grid-row: 2;
			grid-column: 1 / -1;
		}
	}
	.help summary {
		color: var(--accent-strong);
		font-size: 12.5px;
		font-weight: 600;
		cursor: pointer;
	}
	.help p {
		color: var(--text-muted);
		font-size: 12px;
		line-height: 1.5;
	}
	.small {
		margin: 0;
		font-size: 11.5px;
	}
	code {
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
	}
	.grafana {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
		margin: 0;
		padding: 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	legend {
		padding: 0 4px;
		font-size: 12.5px;
		font-weight: 600;
	}
</style>

<script lang="ts">
	import type { Integration, IntegrationSettings } from '../api';
	import { fitsText, templateFromUrl, titleFromUrl } from '../grafana';
	import { openTool } from '../integrations';
	import { app } from '../state.svelte';
	import DashboardList from './DashboardList.svelte';
	import PageHeader from './PageHeader.svelte';

	/** What each tool adds, and the kinds to open for the operators. */
	const ABOUT: Record<string, { text: string; install?: string; kinds?: { group: string; plural: string; label: string }[] }> = {
		prometheus: { install: 'https://github.com/prometheus-community/helm-charts/tree/main/charts/kube-prometheus-stack', text: 'Charts of CPU, memory, restarts, network and requests for pods, workloads and nodes (m), and an hour of history in the overview.' },
		grafana: { install: 'https://grafana.com/docs/grafana/latest/setup-grafana/installation/kubernetes/', text: 'Links from objects to the dashboards that show them.' },
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
	const EMPTY: IntegrationSettings = { prometheus: '', grafana: '', grafanaToken: '', loki: '', grafanaLinks: [] };
	let draft = $state<IntegrationSettings>({ ...EMPTY });
	const saved = $derived({ ...EMPTY, ...app.contextSettings.integrations });
	const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(saved));

	$effect(() => {
		// A deep copy: editing a link must not change the saved one.
		draft = JSON.parse(JSON.stringify(saved));
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
			<ul class="tools">
				{#each app.integrations.items as item (item.id)}
					{@const s = status(item)}
					{@const about = ABOUT[item.id]}
					<li>
						<div class="head">
							<strong>{item.name}</strong>
							<span class="status {s.tone}">{s.text}</span>
						</div>
						<p class="about">{about?.text}</p>
						{#if item.source === 'missing' && about?.install}
							<p class="where"><a href={about.install} target="_blank" rel="noreferrer">How to install {item.name}</a></p>
						{/if}
						{#if where(item)}<p class="where">{where(item)}</p>{/if}
						{#if item.error}<p class="error">{item.error}</p>{/if}
						{#if item.id === 'grafana' && item.endpoint && !item.error}
							<div class="kinds">
								<button class="btn" onclick={() => openTool('grafana', '/')}>Open Grafana</button>
							</div>
							{#if namespaceRef}
								<div class="dashboards">
									<DashboardList target={namespaceRef} title="Dashboards for {app.namespace}" empty="Grafana has no dashboard for {app.namespace} alone." />
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

		<form class="settings" onsubmit={save}>
			<h2>Point elsewhere</h2>
			<p class="muted">
				For {app.contextName(context)} only. Leave a field empty to find the tool in the cluster, or type <code>off</code> to turn it off. A
				URL like <code>http://name.namespace.svc:port</code> reaches that Service through the API server.
			</p>
			{#each [URLS.prometheus, URLS.loki] as url (url!.key)}
				<label>
					<span>{url!.key === 'prometheus' ? 'Prometheus' : 'Loki'} URL</span>
					<input bind:value={draft[url!.key]} placeholder={url!.example} spellcheck="false" autocomplete="off" />
				</label>
			{/each}
			<fieldset class="grafana">
				<legend>Grafana</legend>
				<label>
					<span>URL</span>
					<input bind:value={draft.grafana} placeholder={URLS.grafana!.example} spellcheck="false" autocomplete="off" />
				</label>
				<p class="muted small">
					With a URL, <strong>Grafana…</strong> on any object searches Grafana for its name in your browser, also when Grafana needs a login or a
					client certificate.
				</p>
				<label>
					<span>Token</span>
					<input type="password" bind:value={draft.grafanaToken} placeholder="A service account token with the Viewer role" autocomplete="off" />
				</label>
				<p class="muted small">Lets Kubri list Grafana’s dashboards itself. It goes only to a Grafana given by URL, and is kept in Kubri’s settings file.</p>

				<span class="label">Dashboards for objects</span>
				<p class="muted small">
					Paste the address of a dashboard from your browser. Kubri puts the namespace, pod, node or workload you pick in its place, so
					<strong>Grafana…</strong> opens that dashboard set to the object.
				</p>
				{#each draft.grafanaLinks as link, i (i)}
					<div class="link-row">
						<input class="title" bind:value={link.title} placeholder="Title" aria-label="Title of dashboard {i + 1}" spellcheck="false" autocomplete="off" />
						<input
							class="url"
							bind:value={link.url}
							onchange={() => {
								link.url = templateFromUrl(link.url);
								if (!link.title.trim()) link.title = titleFromUrl(link.url);
							}}
							onpaste={(e) => {
								const text = e.clipboardData?.getData('text') ?? '';
								const input = e.currentTarget;
								// Only a whole address replaces the field; pasting into a part of it stays a plain paste.
								const whole = !input.value.trim() || (input.selectionStart === 0 && input.selectionEnd === input.value.length);
								if (!text || !whole) return;
								e.preventDefault();
								link.url = templateFromUrl(text);
								if (!link.title.trim()) link.title = titleFromUrl(link.url);
							}}
							placeholder="https://grafana.example.com/d/uid/pods?var-namespace=shop&var-pod=web-1"
							aria-label="Address of dashboard {i + 1}"
							spellcheck="false"
							autocomplete="off"
						/>
						<button class="btn small" type="button" onclick={() => draft.grafanaLinks.splice(i, 1)} aria-label="Remove dashboard {i + 1}">Remove</button>
						{#if link.url.trim()}<span class="fits">{fitsText(link.url)}</span>{/if}
					</div>
				{/each}
				<div>
					<button class="btn small" type="button" onclick={() => draft.grafanaLinks.push({ title: '', url: '' })}>Add dashboard</button>
				</div>
				<details class="help">
					<summary>Placeholders</summary>
					<p class="muted small">
						Kubri replaces <code>var-namespace</code>, <code>var-pod</code>, <code>var-node</code>, <code>var-workload</code>,
						<code>var-deployment</code> and the like on its own. You can also write them yourself: <code>{'{namespace}'}</code>,
						<code>{'{name}'}</code>, <code>{'{kind}'}</code>, <code>{'{context}'}</code>, the name by kind like <code>{'{pod}'}</code> or
						<code>{'{node}'}</code>, and <code>{'{workload}'}</code>. A dashboard shows only for objects that fill all of its placeholders. A
						path like <code>/d/uid?var-pod={'{pod}'}</code> opens on the Grafana above.
					</p>
				</details>
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
	.label {
		margin-top: 6px;
		font-size: 12.5px;
	}
	.fits {
		grid-column: 1 / -1;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.help summary {
		cursor: pointer;
		font-size: 12px;
		color: var(--accent-strong);
	}
	.help p {
		margin-top: 6px;
	}
	.link-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 4px 6px;
		align-items: center;
	}
	.link-row .url {
		grid-column: 1;
	}
	.link-row .btn {
		grid-row: 1 / span 2;
		grid-column: 2;
	}
</style>

<script lang="ts">
	import { tick } from 'svelte';
	import { backend, type BenchInfo } from '../api';
	import { bytes, duration, failureShare, pathOf, progress, rate, statusTone } from '../bench';
	import { age, errorMessage } from '../format';
	import { forControl } from '../keys';
	import { app } from '../state.svelte';
	import Histogram from './Histogram.svelte';

	let now = $state(Date.now());
	let list = $state<HTMLDivElement>();
	let showTable = $state(false);

	const runs = $derived(app.benchmarks);
	const running = $derived(runs.some((r) => r.state === 'running'));
	const selected = $derived(runs.find((r) => r.id === app.benchmarkId) ?? runs[0]);

	// Poll fast while something runs, slowly otherwise.
	$effect(() => {
		const fast = running;
		app.refreshBenchmarks();
		const timer = setInterval(
			() => {
				now = Date.now();
				app.refreshBenchmarks();
			},
			fast ? 500 : 3000
		);
		return () => clearInterval(timer);
	});

	$effect(() => {
		void app.benchmarkId;
		tick().then(() => list?.querySelector('.run.active')?.scrollIntoView({ block: 'nearest' }));
	});

	function move(delta: number) {
		if (!runs.length) return;
		const i = Math.max(0, runs.findIndex((r) => r.id === selected?.id));
		app.benchmarkId = runs[Math.max(0, Math.min(runs.length - 1, i + delta))].id;
	}

	async function stop(run: BenchInfo) {
		try {
			await backend.stopBenchmark(run.id);
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
		app.refreshBenchmarks();
	}

	async function remove(run: BenchInfo) {
		try {
			await backend.removeBenchmark(run.id);
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
		await app.refreshBenchmarks();
	}

	function rerun(run: BenchInfo) {
		const forward = app.forwards.find((f) => f.id === run.forwardId);
		if (forward) app.dialog = { kind: 'benchmark', forward, preset: run };
		else app.toast('Its port-forward is no longer running', 'bad');
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || forControl(e)) return;
		const ctrl = e.ctrlKey || e.metaKey;
		const run = selected;
		const handled = (() => {
			if ((ctrl && e.key.toLowerCase() === 'd') || (!ctrl && e.key === 'Delete')) {
				if (run) (run.state === 'running' ? stop : remove)(run);
				return true;
			}
			if (ctrl || e.altKey) return false;
			switch (e.key) {
				case 'ArrowDown':
				case 'j':
					move(1);
					return true;
				case 'ArrowUp':
				case 'k':
					move(-1);
					return true;
				case 'b':
					if (run) rerun(run);
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}

	function share(n: number, total: number) {
		return total ? `${Math.round((100 * n) / total)}%` : '0%';
	}

	function plan(run: BenchInfo) {
		const amount = run.requests ? `${run.requests.toLocaleString('en-US')} requests` : `${run.durationSecs}s`;
		return `${run.concurrency} concurrent · ${amount}`;
	}
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<h1>Benchmarks</h1>
		<span class="count">{runs.length}</span>
		<span class="spacer"></span>
		<span class="muted hint">Start one from a port-forward with <kbd>b</kbd>.</span>
	</header>

	{#if runs.length}
		<div class="split">
			<div class="list" bind:this={list} role="listbox" aria-label="Benchmarks">
				{#each runs as run (run.id)}
					<button
						class="run"
						class:active={run.id === selected?.id}
						role="option"
						aria-selected={run.id === selected?.id}
						onclick={() => (app.benchmarkId = run.id)}
					>
						<span class="line">
							<span class="method">{run.method}</span>
							<span class="path">{pathOf(run.url)}</span>
						</span>
						<span class="line muted">
							<span class="pod">{run.pod}</span>
							<span>{age(run.started, now)}</span>
						</span>
						{#if run.state === 'running'}
							<span class="progress"><span style:width="{progress(run) * 100}%"></span></span>
						{:else if run.results}
							<span class="line small">
								<span>{rate(run.results.requestsPerSecond)} req/s · p95 {duration(run.results.percentiles.find((p) => p.percent === 95)?.seconds)}</span>
								{#if run.state === 'stopped'}<span class="muted">stopped</span>{/if}
							</span>
						{/if}
					</button>
				{/each}
			</div>

			<div class="detail">
				{#if selected}
					{@const r = selected.results}
					<div class="title">
						<span class="method">{selected.method}</span>
						<h2 title={selected.url}>{selected.url}</h2>
					</div>
					<p class="muted facts">
						{selected.target.resource.plural}/{selected.target.name} → pod {selected.pod} · {plan(selected)} · started
						{age(selected.started, now)} ago
					</p>
					<div class="actions">
						{#if selected.state === 'running'}
							<button onclick={() => stop(selected)} title="Stop (Ctrl+D)">Stop</button>
						{:else}
							<button onclick={() => rerun(selected)} title="Run again (b)">Run again…</button>
							<button onclick={() => remove(selected)} title="Remove (Ctrl+D)">Remove</button>
						{/if}
					</div>

					{#if selected.state === 'running'}
						<div class="live">
							<span class="progress big"><span style:width="{progress(selected) * 100}%"></span></span>
							<span>
								{selected.completed.toLocaleString('en-US')}{selected.requests ? ` of ${selected.requests.toLocaleString('en-US')}` : ''} requests
								· {selected.elapsed.toFixed(1)}s{selected.durationSecs ? ` of ${selected.durationSecs}s` : ''}
								· about {rate(selected.elapsed ? selected.completed / selected.elapsed : 0)} req/s
							</span>
						</div>
					{:else if r}
						{@const p = (n: number) => r.percentiles.find((x) => x.percent === n)?.seconds}
						{@const failed = failureShare(selected)}
						<div class="tiles">
							<div class="tile"><span class="label">Requests per second</span><span class="value">{rate(r.requestsPerSecond)}</span></div>
							<div class="tile"><span class="label">Average</span><span class="value">{duration(r.average)}</span></div>
							<div class="tile"><span class="label">Median</span><span class="value">{duration(p(50))}</span></div>
							<div class="tile"><span class="label">95th percentile</span><span class="value">{duration(p(95))}</span></div>
							<div class="tile"><span class="label">99th percentile</span><span class="value">{duration(p(99))}</span></div>
							<div class="tile" class:bad={failed > 0}>
								<span class="label">Failed</span>
								<span class="value">{share(Math.round(failed * r.total), r.total)}</span>
							</div>
						</div>

						<div class="panels">
							<div class="panel wide">
								<div class="panel-head">
									<h3>Response times</h3>
									<span class="spacer"></span>
									<button class="link" onclick={() => (showTable = !showTable)}>{showTable ? 'Chart' : 'Table'}</button>
								</div>
								{#if !r.histogram.length}
									<p class="muted">No responses to chart.</p>
								{:else if showTable}
									<table>
										<thead><tr><th>From</th><th>To</th><th class="num">Responses</th><th class="num">Share</th></tr></thead>
										<tbody>
											{#each r.histogram as b, i (i)}
												<tr><td>{duration(b.from)}</td><td>{duration(b.to)}</td><td class="num">{b.count}</td><td class="num">{share(b.count, r.total)}</td></tr>
											{/each}
										</tbody>
									</table>
								{:else}
									<Histogram buckets={r.histogram} total={r.total} />
								{/if}
							</div>

							<div class="panel">
								<h3>Percentiles</h3>
								<table>
									<tbody>
										{#each r.percentiles as x (x.percent)}
											<tr><td>{x.percent}%</td><td class="num">{duration(x.seconds)}</td></tr>
										{/each}
										<tr><td>Fastest</td><td class="num">{duration(r.fastest)}</td></tr>
										<tr><td>Slowest</td><td class="num">{duration(r.slowest)}</td></tr>
									</tbody>
								</table>
							</div>

							<div class="panel">
								<h3>Status codes</h3>
								{#if r.statusCodes.length}
									<ul class="codes">
										{#each r.statusCodes as s (s.code)}
											<li>
												<span class="code {statusTone(s.code)}">{s.code}</span>
												<span class="bar"><span class={statusTone(s.code)} style:width={share(s.count, r.total)}></span></span>
												<span class="num">{s.count.toLocaleString('en-US')}</span>
											</li>
										{/each}
									</ul>
								{:else}
									<p class="muted">No responses.</p>
								{/if}
								{#if r.errors.length}
									<h3>Errors</h3>
									<ul class="errors">
										{#each r.errors as err (err.message)}
											<li><span class="num">{err.count.toLocaleString('en-US')}×</span> {err.message}</li>
										{/each}
									</ul>
								{/if}
							</div>

							<div class="panel">
								<h3>Totals</h3>
								<table>
									<tbody>
										<tr><td>Requests</td><td class="num">{r.total.toLocaleString('en-US')}</td></tr>
										<tr><td>Duration</td><td class="num">{duration(r.duration)}</td></tr>
										<tr><td>Data received</td><td class="num">{bytes(r.bytes)}</td></tr>
										<tr><td>Per response</td><td class="num">{r.total ? bytes(r.bytes / r.total) : '—'}</td></tr>
									</tbody>
								</table>
							</div>
						</div>
					{/if}
				{/if}
			</div>
		</div>
	{:else}
		<div class="empty">
			<p>No benchmarks yet.</p>
			<p class="muted">
				Start a port-forward, then press <kbd>b</kbd> on it in <button class="link" onclick={() => app.showPortForwards()}>Port-forwards</button>
				to send it a burst of HTTP requests and see how fast it answers.
			</p>
		</div>
	{/if}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		container-type: inline-size;
	}
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.hint {
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.split {
		flex: 1;
		display: grid;
		grid-template-columns: minmax(220px, 290px) minmax(0, 1fr);
		min-height: 0;
		border-top: 1px solid var(--border-subtle);
	}
	@container (max-width: 760px) {
		.split {
			grid-template-columns: minmax(0, 1fr);
			grid-template-rows: minmax(0, 1fr) minmax(0, 3fr);
		}
		.list {
			border-right: none;
			border-bottom: 1px solid var(--border-subtle);
		}
	}
	.list {
		overflow-y: auto;
		padding: 8px;
		border-right: 1px solid var(--border-subtle);
	}
	.run {
		display: flex;
		flex-direction: column;
		gap: 3px;
		width: 100%;
		padding: 8px 10px;
		border: none;
		border-radius: 8px;
		background: none;
		text-align: left;
	}
	.run:hover {
		background: var(--surface-2);
	}
	.run.active {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
	}
	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
		font-size: 12.5px;
	}
	.line > :last-child:not(:first-child) {
		margin-left: auto;
	}
	.small {
		font-size: 11.5px;
	}
	.method {
		padding: 0 5px;
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
		font-weight: 600;
	}
	.path,
	.pod {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--mono);
	}
	.progress {
		display: block;
		height: 4px;
		margin-top: 2px;
		border-radius: 2px;
		background: color-mix(in srgb, var(--accent) 16%, var(--surface-2));
		overflow: hidden;
	}
	.progress > span {
		display: block;
		height: 100%;
		border-radius: 2px;
		background: var(--accent);
		transition: width 0.4s;
	}
	.progress.big {
		height: 8px;
		border-radius: 4px;
	}
	.detail {
		overflow: auto;
		padding: 14px 20px 24px;
	}
	.title {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	h2 {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--mono);
		font-size: 15px;
	}
	.facts {
		margin: 6px 0 10px;
		font-size: 12.5px;
	}
	.actions {
		display: flex;
		gap: 8px;
		margin-bottom: 14px;
	}
	.actions button {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.live {
		display: flex;
		flex-direction: column;
		gap: 8px;
		max-width: 560px;
		font-size: 13px;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
		gap: 10px;
		margin-bottom: 14px;
	}
	.tile {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	.tile.bad {
		box-shadow: inset 3px 0 0 var(--bad);
	}
	.label {
		color: var(--text-muted);
		font-size: 11.5px;
	}
	.value {
		font-size: 20px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.panels {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 12px;
	}
	.panel {
		padding: 12px 14px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	.panel.wide {
		grid-column: 1 / -1;
	}
	.panel-head {
		display: flex;
		align-items: center;
	}
	h3 {
		margin: 0 0 8px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.panel h3 + table,
	.panel h3 + ul {
		margin-top: 0;
	}
	.panel ul + h3 {
		margin-top: 14px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12.5px;
	}
	th {
		padding: 4px 6px;
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 600;
		text-align: left;
	}
	td {
		padding: 3px 6px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.codes,
	.errors {
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: 12.5px;
	}
	.codes li {
		display: grid;
		grid-template-columns: 40px 1fr auto;
		align-items: center;
		gap: 8px;
		padding: 3px 0;
	}
	.code {
		font-family: var(--mono);
		font-weight: 600;
	}
	.bar {
		height: 8px;
		border-radius: 4px;
		background: var(--surface-2);
		overflow: hidden;
	}
	.bar > span {
		display: block;
		height: 100%;
		border-radius: 4px;
	}
	.bar .ok {
		background: var(--ok);
	}
	.bar .muted {
		background: var(--text-muted);
	}
	.bar .warn {
		background: var(--warn-mark);
	}
	.bar .bad {
		background: var(--bad);
	}
	.errors li {
		padding: 3px 0;
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font: inherit;
		font-size: 12px;
		text-decoration: underline;
	}
	.empty {
		padding: 40px 24px;
		text-align: center;
	}
	.empty p {
		margin: 4px 0;
	}
	kbd {
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
	}
</style>

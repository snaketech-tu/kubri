// Measures how fast Kubri's web server starts, how much memory it and the page use, and how long
// the pod list takes to show its first pods and every pod, and does the same for Headlamp when it is
// there. Headlamp loads 1,000 pods at a time, so the script presses its "Load more" button.
//
//   node bench/speed/run.cjs
//
// It reads the cluster from KUBECONFIG, which bench/speed/seed.sh fills with pods first.
//   KUBRI_BIN       kubri-server to run (target/release/kubri-server)
//   KUBRI_UI_DIR    the built UI (ui/build)
//   HEADLAMP_DIR    a folder with headlamp-server, frontend/ and plugins/; Headlamp is skipped without it
//   BENCH_PODS      pods the list must reach (10000)
//   BENCH_RUNS      runs per app; the median counts (3)
//   BENCH_OUT       where to write the results as JSON (speed.json)
// Kubri's medians are checked against bench/speed/limits.json, and the script fails when one is over.
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const pods = Number(process.env.BENCH_PODS || 10000);
const runs = Number(process.env.BENCH_RUNS || 3);
const out = process.env.BENCH_OUT || 'speed.json';
const limits = JSON.parse(fs.readFileSync(path.join(__dirname, 'limits.json'), 'utf8'));
const context = execFileSync('kubectl', ['config', 'current-context'], { encoding: 'utf8' }).trim();
const kubeconfig = (process.env.KUBECONFIG || path.join(os.homedir(), '.kube/config')).split(path.delimiter)[0];

const apps = [
	{
		name: 'Kubri',
		key: 'kubri',
		port: 7979,
		url: (port) => `http://127.0.0.1:${port}/`,
		start(port) {
			// Its own settings, so nothing from the machine's Kubri or k9s changes the page. Like k9s,
			// it opens the pod list first, as Headlamp's address does.
			const home = fs.mkdtempSync(path.join(os.tmpdir(), 'kubri-bench-'));
			fs.mkdirSync(path.join(home, 'k9s'));
			fs.writeFileSync(path.join(home, 'k9s', 'config.yaml'), 'k9s:\n  defaultView: pods\n');
			return spawn(process.env.KUBRI_BIN || 'target/release/kubri-server', ['--addr', `127.0.0.1:${port}`], {
				env: {
					...process.env,
					KUBECONFIG: kubeconfig,
					KUBRI_UI_DIR: process.env.KUBRI_UI_DIR || 'ui/build',
					KUBRI_CONFIG_DIR: path.join(home, 'kubri'),
					K9S_CONFIG_DIR: path.join(home, 'k9s')
				},
				stdio: 'ignore'
			});
		}
	},
	process.env.HEADLAMP_DIR && {
		name: 'Headlamp',
		key: 'headlamp',
		port: 4466,
		url: (port) => `http://127.0.0.1:${port}/c/${encodeURIComponent(context)}/pods`,
		// Headlamp lists pods 1,000 at a time and asks for the rest with a button.
		more: (page) => page.getByRole('button', { name: 'Load more' }),
		start(port) {
			const dir = process.env.HEADLAMP_DIR;
			const args = ['-html-static-dir', path.join(dir, 'frontend'), '-plugins-dir', path.join(dir, 'plugins')];
			args.push('-kubeconfig', kubeconfig, '-port', String(port));
			return spawn(path.join(dir, 'headlamp-server'), args, { stdio: 'ignore' });
		}
	}
].filter(Boolean);

const now = () => performance.now();
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

/** The highest resident memory of a process so far, in MB. */
function peakMemory(pid) {
	const status = fs.readFileSync(`/proc/${pid}/status`, 'utf8');
	return Number(/VmHWM:\s+(\d+) kB/.exec(status)[1]) / 1024;
}

/** Waits until the server answers, and returns how long that took from `started`. */
async function ready(url, started, child) {
	while (now() - started < 60_000) {
		if (child.exitCode !== null) throw new Error(`the server exited with ${child.exitCode}`);
		try {
			if ((await fetch(url)).ok) return now() - started;
		} catch {
			// Not listening yet.
		}
		await pause(5);
	}
	throw new Error(`${url} did not answer within a minute`);
}

/** True once a seeded pod, named like api-00042, is on the page. */
function shown() {
	return /-\d{5}\b/.test(document.body.innerText);
}

/** True once the page says it shows `of N` items with N at least `pods`, like "10012 of 10012 pods" or "1-15 of 10012". */
function listed(pods) {
	for (const [, n] of document.body.innerText.matchAll(/\bof ([\d,.]+)/g)) {
		if (Number(n.replace(/[,.]/g, '')) >= pods) return true;
	}
	return false;
}

async function measure(app, browser) {
	const child = app.start(app.port);
	const started = now();
	try {
		const startup = await ready(`http://127.0.0.1:${app.port}/`, started, child);
		const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
		const cdp = await page.context().newCDPSession(page);
		await cdp.send('Performance.enable');
		const opened = now();
		await page.goto(app.url(app.port));
		const deadline = opened + 180_000;
		await page.waitForFunction(shown, null, { timeout: 180_000, polling: 50 });
		const first = now() - opened;
		// Pressing a "Load more" button whenever it can be pressed, as fast as a person never would.
		while (!(await page.evaluate(listed, pods))) {
			if (now() > deadline) {
				await page.screenshot({ path: `speed-${app.key}.png` });
				const text = await page.evaluate(() => document.body.innerText).catch(() => '');
				throw new Error(`not all pods listed after 180 s; the page shows:\n${text.slice(0, 1500)}`);
			}
			const more = app.more?.(page);
			// Only a button that is there now: Playwright waits for a missing one, and a timeout of 0 waits forever.
			if (more && (await more.count()) === 1 && (await more.isEnabled().catch(() => false))) await more.click({ timeout: 1000 }).catch(() => {});
			await pause(50);
		}
		const list = now() - opened;
		// Settled memory: what the page keeps once the list is up, not garbage waiting to go.
		await pause(1000);
		await cdp.send('HeapProfiler.collectGarbage');
		const { metrics } = await cdp.send('Performance.getMetrics');
		const heap = metrics.find((m) => m.name === 'JSHeapUsedSize').value / 1024 / 1024;
		const server = peakMemory(child.pid);
		await page.close();
		return { startupMs: startup, firstMs: first, listMs: list, serverMemoryMb: server, browserHeapMb: heap };
	} finally {
		child.kill('SIGTERM');
		await new Promise((resolve) => (child.exitCode !== null ? resolve() : child.once('exit', resolve)));
	}
}

const fields = [
	['startupMs', 'Server start', (v) => `${Math.round(v)} ms`],
	['firstMs', 'Page open until the first pods show', (v) => `${(v / 1000).toFixed(2)} s`],
	['listMs', `Page open until all ${pods.toLocaleString('en')} pods are listed`, (v) => `${(v / 1000).toFixed(2)} s`],
	['serverMemoryMb', 'Server memory, peak', (v) => `${Math.round(v)} MB`],
	['browserHeapMb', 'Page memory (JavaScript heap)', (v) => `${Math.round(v)} MB`]
];

(async () => {
	const browser = await chromium.launch();
	const results = {};
	for (const app of apps) {
		const samples = [];
		for (let i = 0; i < runs; i++) {
			try {
				samples.push(await measure(app, browser));
			} catch (err) {
				console.error(`${app.name}, run ${i + 1}: ${err.message}`);
			}
		}
		if (!samples.length) continue;
		results[app.key] = { name: app.name, samples };
		for (const [key] of fields) results[app.key][key] = median(samples.map((s) => s[key]));
	}
	await browser.close();

	const shown = Object.values(results);
	const failed = fields.filter(([key]) => !results.kubri || results.kubri[key] > limits.kubri[key]);
	const lines = [
		`| | ${shown.map((r) => r.name).join(' | ')} | Kubri limit |`,
		`| --- | ${shown.map(() => '---').join(' | ')} | --- |`,
		...fields.map(
			([key, label, show]) =>
				`| ${label} | ${shown.map((r) => show(r[key])).join(' | ')} | ${show(limits.kubri[key])}${failed.some(([k]) => k === key) ? ' ❌' : ''} |`
		),
		'',
		`Median of ${runs} runs, ${pods.toLocaleString('en')} pods, ${os.cpus().length} CPUs, ${new Date().toISOString().slice(0, 10)}.`
	];
	console.log(lines.join('\n'));
	if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Speed\n\n${lines.join('\n')}\n`);
	fs.writeFileSync(out, JSON.stringify({ pods, runs, limits: limits.kubri, results }, null, 2));

	if (!results.kubri) {
		console.error('Kubri could not be measured.');
		process.exit(1);
	}
	if (failed.length) {
		console.error(`Slower or bigger than the limit: ${failed.map(([, label]) => label).join(', ')}.`);
		process.exit(1);
	}
})();

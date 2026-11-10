// Records what kubri-server answers while a browser looks around the demo snapshot, for the
// live demo: the web app built with VITE_KUBRI_DEMO answers from this recording without a
// server (ui/src/lib/api/replay.ts).
//
//   node scripts/demo/record.cjs <path-to-playwright-module> <recording.json>
//
// KUBRI_BIN (target/release/kubri-server), KUBRI_UI_DIR (ui/build) and DEMO_SNAPSHOT
// (ui/static/demo.kubri) can point elsewhere.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');

const root = path.resolve(__dirname, '../..');
const [playwrightModule = 'playwright', out = path.join(root, 'ui/build/demo-recording.json')] = process.argv.slice(2);
const { chromium } = require(playwrightModule);

/** Kinds whose objects get opened one by one, with every tab of the inspector. */
const OPEN = new Set([
	'pods',
	'deployments',
	'statefulsets',
	'daemonsets',
	'replicasets',
	'jobs',
	'cronjobs',
	'services',
	'ingresses',
	'configmaps',
	'secrets',
	'serviceaccounts',
	'roles',
	'rolebindings',
	'horizontalpodautoscalers',
	'poddisruptionbudgets',
	'networkpolicies',
	'persistentvolumeclaims',
	'nodes',
	'namespaces',
	'storageclasses',
	'customresourcedefinitions'
]);
/** Pages besides the lists, by their commands, each in the namespace and in all of them. */
const PAGES = ['pulses', 'graph', 'rbac', 'helm', 'popeye', 'rightsizing', 'integrations', 'changes', 'config', 'pf', 'bench', 'search', 'new'];
const NAMESPACE = 'shop';

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

function freePort() {
	return new Promise((resolve, reject) => {
		const server = net.createServer();
		server.listen(0, '127.0.0.1', () => {
			const { port } = server.address();
			server.close(() => resolve(port));
		});
		server.on('error', reject);
	});
}

async function startServer() {
	const port = await freePort();
	const home = fs.mkdtempSync(path.join(os.tmpdir(), 'kubri-demo-'));
	fs.mkdirSync(path.join(home, 'k9s'));
	const child = spawn(
		process.env.KUBRI_BIN ?? path.join(root, 'target/release/kubri-server'),
		['--addr', `127.0.0.1:${port}`, '--snapshot', process.env.DEMO_SNAPSHOT ?? path.join(root, 'ui/static/demo.kubri')],
		{
			env: {
				...process.env,
				// Only the snapshot: nothing on this computer shows in the demo.
				KUBECONFIG: path.join(home, 'no-kubeconfig'),
				KUBRI_CONFIG_DIR: path.join(home, 'kubri'),
				K9S_CONFIG_DIR: path.join(home, 'k9s'),
				KUBRI_UI_DIR: process.env.KUBRI_UI_DIR ?? path.join(root, 'ui/build'),
				RUST_LOG: 'warn'
			},
			stdio: ['ignore', 'inherit', 'inherit']
		}
	);
	// Ends with this script, also when it fails.
	process.on('exit', () => child.kill());
	const url = `http://127.0.0.1:${port}`;
	for (let i = 0; i < 200; i++) {
		try {
			if ((await fetch(`${url}/`)).ok) return { url, stop: () => child.kill() };
		} catch {
			// Not up yet.
		}
		await pause(100);
	}
	child.kill();
	throw new Error('kubri-server did not start');
}

/** Records the page's requests to /api and waits until none is under way. */
async function recorder(page) {
	const recording = { version: 1, http: [], sockets: [] };
	let pending = 0;
	let changed = Date.now();
	const touch = () => (changed = Date.now());

	await page.route(/\/api\//, async (route) => {
		pending++;
		touch();
		try {
			const request = route.request();
			const response = await route.fetch();
			const body = await response.text();
			recording.http.push({
				method: request.method(),
				url: new URL(request.url()).pathname + new URL(request.url()).search,
				body: request.postData(),
				status: response.status(),
				type: response.headers()['content-type'] ?? 'application/json',
				response: body
			});
			await route.fulfill({ status: response.status(), headers: response.headers(), body });
		} catch (err) {
			await route.abort().catch(() => {});
			if (!String(err).includes('closed')) console.warn(`not recorded: ${route.request().url()}: ${err.message}`);
		} finally {
			pending--;
			touch();
		}
	});

	await page.routeWebSocket(/\/api\//, (ws) => {
		const url = new URL(ws.url());
		const entry = { url: url.pathname + url.search, first: null, messages: [], closed: false };
		recording.sockets.push(entry);
		touch();
		const server = ws.connectToServer();
		let sent = false;
		let left = false;
		ws.onMessage((message) => {
			if (!sent) entry.first = typeof message === 'string' ? message : null;
			sent = true;
			server.send(message);
		});
		server.onMessage((message) => {
			entry.messages.push(typeof message === 'string' ? { text: message } : { binary: Buffer.from(message).toString('base64') });
			touch();
			ws.send(message);
		});
		server.onClose(() => {
			// Only when the server ended it, not when the page left.
			if (!left) entry.closed = true;
			ws.close();
		});
		ws.onClose(() => {
			left = true;
			server.close();
		});
	});

	/** Waits until nothing was asked or answered for `quiet` ms. */
	async function settle(quiet = 250, timeout = 15_000) {
		const started = Date.now();
		while (pending > 0 || Date.now() - changed < quiet) {
			if (Date.now() - started > timeout) {
				console.warn(`still busy after ${timeout} ms on ${page.url()}`);
				return;
			}
			await pause(50);
		}
	}

	return { recording, settle };
}

async function main() {
	const server = await startServer();
	const browser = await chromium.launch();
	const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
	await context.addInitScript(() => {
		try {
			localStorage.setItem('kubri.tour', 'seen');
		} catch {
			// The tour shows; the recording does not depend on it.
		}
	});
	const page = await context.newPage();
	page.on('pageerror', (err) => console.warn(`page error: ${err.message}`));
	const { recording, settle } = await recorder(page);

	await page.goto(server.url);
	await page.getByText('Snapshot · ', { exact: false }).first().waitFor({ timeout: 30_000 });
	await settle();
	const started = Date.now();

	/** Runs a command as typed after `:`, like `pods` or `ns shop`, with nothing else open. */
	async function command(text) {
		console.log(`${Math.round((Date.now() - started) / 1000)}s :${text}`);
		for (let i = 0; i < 3; i++) await page.keyboard.press('Escape');
		// Out of fields and editors, where `:` would be typed in.
		await page.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
		await page.keyboard.press(':');
		await page.keyboard.type(text);
		await page.keyboard.press('Enter');
		await settle();
	}

	const isResources = (r) => r.url === '/api/call' && r.status === 200 && JSON.parse(r.body ?? '{}').call === 'resources';
	const resources = JSON.parse(recording.http.findLast(isResources)?.response ?? '[]');
	const listable = resources.filter((r) => (r.verbs ?? ['list']).includes('list'));
	const name = (r) => (r.group ? `${r.plural}.${r.group}` : r.plural);
	console.log(`${listable.length} kinds`);

	for (const namespace of [NAMESPACE, 'all']) {
		await command(`ns ${namespace}`);
		for (const p of PAGES) await command(p);
		for (const r of listable) {
			if (namespace === 'all' && !r.namespaced) continue;
			await command(name(r));
			if (OPEN.has(r.plural)) await command(`xray ${name(r)}`);
		}
	}

	// Every object of the kinds people open first, with each tab of the inspector.
	await command(`ns ${NAMESPACE}`);
	for (const r of listable.filter((r) => OPEN.has(r.plural))) {
		await command(name(r));
		const names = await page.locator('tr.row td.name button').allInnerTexts();
		for (const object of names.slice(0, 25)) {
			const row = page.locator('tr.row td.name button', { hasText: object }).first();
			if (!(await row.count())) continue;
			await row.click();
			const inspector = page.locator('aside[aria-label$=" inspector"]');
			try {
				await inspector.waitFor({ timeout: 5_000 });
			} catch {
				console.warn(`no inspector for ${r.plural}/${object}`);
				continue;
			}
			await settle();
			for (const tab of [/^Events/, /^YAML/, /^Metrics/, /^Logs/, /^Overview/]) {
				const button = inspector.getByRole('tab', { name: tab }).first();
				if (!(await button.count())) continue;
				await button.click();
				await settle(tab.source.includes('Logs') ? 800 : 250);
			}
		}
	}

	// The Helm release, with its revisions.
	await command('helm');
	for (const release of await page.locator('main button', { hasText: /^search$/ }).all()) {
		await release.click();
		await settle();
	}

	await browser.close();
	server.stop();
	// Sockets the page closed before anything happened on them say nothing.
	recording.sockets = recording.sockets.filter((s) => s.first !== null || s.messages.length > 0);
	fs.mkdirSync(path.dirname(out), { recursive: true });
	fs.writeFileSync(out, JSON.stringify(recording));
	console.log(`wrote ${out}: ${recording.http.length} answers, ${recording.sockets.length} sockets, ${fs.statSync(out).size} bytes`);
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});

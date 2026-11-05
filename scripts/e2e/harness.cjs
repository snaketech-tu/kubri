// Helpers for the end-to-end tests: a kubeconfig with two contexts, kubri-server with its own
// settings, a browser page that skips the first-run tour, and kubectl.
//
// Environment:
//   KUBECONFIG          the cluster to test against (scripts/e2e/fixtures.yaml applied)
//   KUBRI_BIN           kubri-server to run (target/release/kubri-server)
//   KUBRI_UI_DIR        the built UI (ui/build)
//   PLAYWRIGHT_MODULE   where to load Playwright from (playwright)
//   E2E_ARTIFACTS       where screenshots of failed tests go (e2e-artifacts)
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const root = path.resolve(__dirname, '../..');
const sourceKubeconfig = (process.env.KUBECONFIG || path.join(os.homedir(), '.kube/config')).split(path.delimiter)[0];
const artifacts = path.resolve(process.env.E2E_ARTIFACTS || path.join(root, 'e2e-artifacts'));
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'kubri-e2e-'));
const kubeconfig = path.join(work, 'kubeconfig');

/** Runs kubectl against the test kubeconfig and returns its output. */
function kubectl(...args) {
	return execFileSync('kubectl', ['--kubeconfig', kubeconfig, ...args], {
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe']
	});
}

/**
 * A copy of the cluster's kubeconfig with two contexts on the same cluster: `e2e-one` in the
 * namespace e2e (the current one) and `e2e-two` in kube-system.
 */
function makeKubeconfig() {
	fs.copyFileSync(sourceKubeconfig, kubeconfig);
	const current = kubectl('config', 'current-context').trim();
	const cluster = kubectl('config', 'view', '-o', `jsonpath={.contexts[?(@.name=="${current}")].context.cluster}`);
	const user = kubectl('config', 'view', '-o', `jsonpath={.contexts[?(@.name=="${current}")].context.user}`);
	kubectl('config', 'set-context', 'e2e-one', `--cluster=${cluster}`, `--user=${user}`, '--namespace=e2e');
	kubectl('config', 'set-context', 'e2e-two', `--cluster=${cluster}`, `--user=${user}`, '--namespace=kube-system');
	kubectl('config', 'use-context', 'e2e-one');
}
makeKubeconfig();

/** A TCP port nobody listens on. */
function freePort() {
	return new Promise((resolve, reject) => {
		const server = net.createServer();
		server.unref();
		server.on('error', reject);
		server.listen(0, '127.0.0.1', () => {
			const { port } = server.address();
			server.close(() => resolve(port));
		});
	});
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Starts kubri-server with settings of its own, so nothing from this machine's Kubri or k9s
 * changes what the tests see. Returns `{ url, port, stop, output }`.
 */
async function startServer({ args = [], env = {} } = {}) {
	const port = await freePort();
	const home = fs.mkdtempSync(path.join(work, 'home-'));
	fs.mkdirSync(path.join(home, 'k9s'));
	const output = [];
	const child = spawn(
		process.env.KUBRI_BIN || path.join(root, 'target/release/kubri-server'),
		['--addr', `127.0.0.1:${port}`, ...args],
		{
			env: {
				...process.env,
				KUBECONFIG: kubeconfig,
				KUBRI_UI_DIR: process.env.KUBRI_UI_DIR || path.join(root, 'ui/build'),
				KUBRI_CONFIG_DIR: path.join(home, 'kubri'),
				K9S_CONFIG_DIR: path.join(home, 'k9s'),
				...env
			},
			stdio: ['ignore', 'pipe', 'pipe']
		}
	);
	child.stdout.on('data', (d) => output.push(String(d)));
	child.stderr.on('data', (d) => output.push(String(d)));
	const url = `http://127.0.0.1:${port}`;
	const started = Date.now();
	for (;;) {
		if (child.exitCode !== null) throw new Error(`kubri-server exited with ${child.exitCode}:\n${output.join('')}`);
		try {
			if ((await fetch(`${url}/`)).ok) break;
		} catch {
			// Not listening yet.
		}
		if (Date.now() - started > 30_000) throw new Error(`kubri-server did not answer:\n${output.join('')}`);
		await pause(50);
	}
	return {
		url,
		port,
		output: () => output.join(''),
		async stop() {
			if (child.exitCode !== null) return;
			child.kill('SIGTERM');
			await new Promise((resolve) => (child.exitCode !== null ? resolve() : child.once('exit', resolve)));
		}
	};
}

let browser;
// The page a test used last, for the screenshot of a failure.
let last;
async function launch() {
	browser ??= await chromium.launch();
	return browser;
}

async function closeBrowser() {
	await browser?.close();
	browser = undefined;
}

/**
 * A new page on `url` with the first-run tour marked as seen. Errors thrown in the page are
 * collected in `page.errors`.
 */
async function openPage(url, options = {}) {
	const context = await (await launch()).newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true, ...options });
	await context.addInitScript(() => {
		try {
			localStorage.setItem('kubri.tour', 'seen');
		} catch {
			// The tour shows; tests that need it gone fail on their own.
		}
	});
	const page = await context.newPage();
	page.errors = [];
	page.on('pageerror', (err) => page.errors.push(err.message));
	await page.goto(url);
	last = page;
	return page;
}

/** Types a command into the command palette, like `pods e2e` for `:pods e2e`. */
async function command(page, text) {
	await page.keyboard.press(':');
	await page.keyboard.type(text);
	await page.keyboard.press('Enter');
}

/** Opens a list by its address, waits for its heading and narrows it to rows matching `filter`. */
async function openList(page, base, { context = 'e2e-one', resource, namespace = 'e2e', heading, filter }) {
	last = page;
	// A hash-only change does not load the page again, so start from a blank one.
	await page.goto('about:blank');
	await page.goto(`${base}/#/${context}/resources/${resource}${namespace ? `?ns=${namespace}` : ''}`);
	await page.getByRole('heading', { name: heading, level: 1, exact: true }).waitFor();
	if (filter) {
		await page.getByRole('textbox', { name: /^Filter / }).fill(filter);
		// Enter keeps the filter and moves to the list, where the keys act on the first row.
		await page.keyboard.press('Enter');
		await page.getByRole('row').nth(1).waitFor();
	}
}

/** Waits until `check` returns true, polling every 250 ms. */
async function until(check, { timeout = 30_000, message = 'condition' } = {}) {
	const started = Date.now();
	let last;
	for (;;) {
		try {
			if (await check()) return;
		} catch (err) {
			last = err;
		}
		if (Date.now() - started > timeout) {
			throw new Error(`timed out waiting for ${message}${last ? `: ${last.message}` : ''}`);
		}
		await pause(250);
	}
}

/** Saves a screenshot of the page under the test's name, for the CI artifact. */
async function screenshot(page, name) {
	fs.mkdirSync(artifacts, { recursive: true });
	await page.screenshot({ path: path.join(artifacts, `${name.replace(/[^\w-]+/g, '-')}.png`) }).catch(() => {});
}

module.exports = {
	kubectl,
	kubeconfig,
	work,
	startServer,
	openPage,
	closeBrowser,
	command,
	openList,
	until,
	pause,
	freePort,
	screenshot,
	lastPage: () => last
};

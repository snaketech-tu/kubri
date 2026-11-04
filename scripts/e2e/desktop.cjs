// Starts the desktop app on Linux through tauri-driver and checks that it connects to the cluster
// and shows the overview. See scripts/e2e/README.md.
//
//   xvfb-run node scripts/e2e/desktop.cjs
//
// Environment:
//   KUBECONFIG     the cluster (the first file counts)
//   KUBRI_DESKTOP  the app to start (target/debug/kubri)
//   TAURI_DRIVER   tauri-driver to run (tauri-driver)
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const root = path.resolve(__dirname, '../..');
const app = path.resolve(process.env.KUBRI_DESKTOP || path.join(root, 'target/debug/kubri'));
const port = 4444;
const driverUrl = `http://127.0.0.1:${port}`;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Its own settings and a context named like the web tests'.
const home = fs.mkdtempSync(path.join(os.tmpdir(), 'kubri-desktop-'));
const kubeconfig = path.join(home, 'kubeconfig');
fs.copyFileSync((process.env.KUBECONFIG || path.join(os.homedir(), '.kube/config')).split(path.delimiter)[0], kubeconfig);
const kubectl = (...args) => execFileSync('kubectl', ['--kubeconfig', kubeconfig, ...args], { encoding: 'utf8' }).trim();
kubectl('config', 'rename-context', kubectl('config', 'current-context'), 'e2e-desktop');

async function webdriver(method, url, body) {
	const response = await fetch(`${driverUrl}${url}`, {
		method,
		headers: { 'content-type': 'application/json' },
		body: body ? JSON.stringify(body) : undefined
	});
	const json = await response.json();
	if (!response.ok) throw new Error(`${method} ${url}: ${JSON.stringify(json.value ?? json)}`);
	return json.value;
}

(async () => {
	const driver = spawn(process.env.TAURI_DRIVER || 'tauri-driver', ['--port', String(port)], {
		env: {
			...process.env,
			KUBECONFIG: kubeconfig,
			XDG_CONFIG_HOME: path.join(home, 'config'),
			XDG_DATA_HOME: path.join(home, 'data'),
			K9S_CONFIG_DIR: path.join(home, 'k9s'),
			WEBKIT_DISABLE_DMABUF_RENDERER: '1'
		},
		stdio: 'inherit'
	});
	let session;
	let text = '';
	try {
		for (let i = 0; ; i++) {
			try {
				await fetch(`${driverUrl}/status`);
				break;
			} catch {
				if (i > 100) throw new Error('tauri-driver did not start');
				await pause(100);
			}
		}
		session = (
			await webdriver('POST', '/session', {
				capabilities: { alwaysMatch: { browserName: 'wry', 'tauri:options': { application: app } } }
			})
		).sessionId;
		const started = Date.now();
		for (;;) {
			text = await webdriver('POST', `/session/${session}/execute/sync`, {
				script: 'return document.body.innerText',
				args: []
			});
			if (/Cluster overview/.test(text) && /e2e-desktop/.test(text) && /Ready nodes/.test(text)) break;
			if (Date.now() - started > 60_000) throw new Error(`the overview did not show; the window shows:\n${text.slice(0, 2000)}`);
			await pause(500);
		}
		console.log('The desktop app connected and shows the overview.');
	} catch (err) {
		console.error(err.message);
		process.exitCode = 1;
	} finally {
		if (session) await webdriver('DELETE', `/session/${session}`).catch(() => {});
		driver.kill('SIGTERM');
	}
})();

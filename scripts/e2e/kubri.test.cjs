// End-to-end tests of the web app against a real cluster. See scripts/e2e/README.md.
//
//   kubectl apply -f scripts/e2e/fixtures.yaml
//   node --test scripts/e2e/
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const path = require('node:path');
const { after, before, describe, test } = require('node:test');

const h = require('./harness.cjs');

let server;
let page;

/** A test that leaves a screenshot of the page behind when it fails. */
function check(name, fn, options = {}) {
	test(name, { timeout: 120_000, ...options }, async () => {
		try {
			await fn();
		} catch (err) {
			const shown = h.lastPage();
			if (shown && !shown.isClosed()) await h.screenshot(shown, name);
			throw err;
		}
	});
}

/** The pod of the deployment `web`. */
function webPod() {
	return h.kubectl('get', 'pods', '-n', 'e2e', '-l', 'app=web', '-o', 'jsonpath={.items[0].metadata.name}').trim();
}

before(async () => {
	h.kubectl('rollout', 'status', 'deployment/web', '-n', 'e2e', '--timeout=180s');
	server = await h.startServer();
	page = await h.openPage(server.url);
});

after(async () => {
	if (page?.errors.length) console.error(`Errors in the page:\n${page.errors.join('\n')}`);
	await h.closeBrowser();
	await server?.stop();
});

describe('lists', () => {
	check('lists the pods of a namespace', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods' });
		await page.getByRole('button', { name: webPod(), exact: true }).waitFor();
		await page.getByRole('button', { name: 'db-0', exact: true }).waitFor();
	});

	check('filters by /regex/ and by label', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: '/^db-/' });
		await h.until(async () => (await page.getByRole('row').count()) === 2, { message: 'one row for db-0' });
		await page.getByRole('button', { name: 'db-0', exact: true }).waitFor();

		await page.getByRole('textbox', { name: 'Filter pods' }).fill('-l app=web');
		await page.getByRole('button', { name: webPod(), exact: true }).waitFor();
		await h.until(async () => (await page.getByRole('row').count()) === 2, { message: 'one row for the web pod' });
	});

	check('shows a new object without a reload', async () => {
		await h.openList(page, server.url, { resource: 'configmaps.v1', heading: 'ConfigMaps' });
		h.kubectl('create', 'configmap', 'arrives-live', '-n', 'e2e', '--from-literal=a=b');
		try {
			await page.getByRole('button', { name: 'arrives-live', exact: true }).waitFor();
		} finally {
			h.kubectl('delete', 'configmap', 'arrives-live', '-n', 'e2e', '--ignore-not-found');
		}
		await page.getByRole('button', { name: 'arrives-live', exact: true }).waitFor({ state: 'detached' });
	});
});

describe('keyboard', () => {
	check('a list opens with its first row chosen, and Enter opens that row', async () => {
		await h.openList(page, server.url, { resource: 'deployments.v1.apps', heading: 'Deployments' });
		// The action bar offers what to do with the first row before any key is pressed.
		await page.getByRole('toolbar').getByRole('button', { name: /^Scale/ }).waitFor();
		// Reach Pods in the sidebar with Tab and open it with Enter, as a keyboard user does.
		await page.evaluate(() => document.activeElement?.blur());
		const onPods = () => page.evaluate(() => document.activeElement?.textContent?.trim() === 'Pods');
		for (let i = 0; i < 60; i++) {
			await page.keyboard.press('Tab');
			// The sidebar may be one Tab stop, at the current page.
			const inSidebar = () => !!document.activeElement?.closest('nav .items') && !!document.querySelector('nav .items [tabindex="-1"]');
			if ((await onPods()) || (await page.evaluate(inSidebar))) break;
		}
		// There the arrow keys move on to Pods.
		for (let i = 0; i < 10 && !(await onPods()); i++) await page.keyboard.press('ArrowUp');
		assert.ok(await onPods(), 'the focus on Pods in the sidebar');
		await page.keyboard.press('Enter');
		await page.getByRole('heading', { name: 'Pods', level: 1, exact: true }).waitFor();
		// The list has the keys now: once its first row is there, Enter opens the pod, not the Pods list again.
		await page.locator('tr.cursor').waitFor();
		await page.keyboard.press('Enter');
		await page.getByRole('complementary', { name: 'Pod inspector' }).waitFor();
	});
});

describe('contexts and addresses', () => {
	check('switches the context and keeps it in the address', async () => {
		await h.openList(page, server.url, { resource: 'deployments.v1.apps', heading: 'Deployments' });
		await h.command(page, 'ctx e2e-two');
		await page.getByRole('button', { name: 'Context e2e-two' }).waitFor();
		await h.until(() => page.url().includes('#/e2e-two/'), { message: 'the address to name e2e-two' });

		await page.reload();
		await page.getByRole('button', { name: 'Context e2e-two' }).waitFor();
		await page.getByRole('heading', { name: 'Deployments', level: 1 }).waitFor();

		await h.command(page, 'ctx e2e-one');
		await page.getByRole('button', { name: 'Context e2e-one' }).waitFor();
	});

	check('goes back with the browser', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods' });
		// Kubri has taken over the browser's Back once its history marker is in place.
		await page.waitForFunction(() => history.state?.kubri === 'now');
		await h.command(page, 'svc');
		await page.getByRole('heading', { name: 'Services', level: 1 }).waitFor();
		await page.goBack();
		await page.getByRole('heading', { name: 'Pods', level: 1 }).waitFor();
	});
});

describe('logs and shell', () => {
	check('streams the logs of every container of a pod', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: 'web' });
		await page.keyboard.press('l');
		const inspector = page.getByRole('complementary', { name: 'Pod inspector' });
		await inspector.getByText('web started').first().waitFor();
		await inspector.getByText(/sidecar line \d+/).first().waitFor();
		// Following: a line written after the stream started shows up.
		const before = Number((await inspector.getByText(/sidecar line \d+/).last().innerText()).match(/\d+/)[0]);
		await h.until(
			async () => Number((await inspector.getByText(/sidecar line \d+/).last().innerText()).match(/\d+/)[0]) > before + 1,
			{ message: 'new log lines' }
		);
	});

	check('runs a command in a shell', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: 'web' });
		await page.keyboard.press('s');
		const rows = page.locator('.xterm-rows');
		await h.until(async () => /[#$] $/m.test((await rows.innerText()).trimEnd() + ' '), { message: 'a prompt' });
		await page.getByRole('textbox', { name: 'Terminal input' }).focus();
		await page.keyboard.type('echo e2e-$((40+2))\n');
		await h.until(async () => (await rows.innerText()).includes('e2e-42'), { message: 'the output of echo' });
		assert.ok(!(await rows.innerText()).includes('kubri-shell'), 'the shell process id stays hidden');
	});

	check('ends a shell when it is closed', async () => {
		const pod = webPod();
		const running = () => h.kubectl('exec', '-n', 'e2e', pod, '-c', 'web', '--', 'ps', '-o', 'args').includes('sleep 4242');
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: 'web' });
		await page.keyboard.press('s');
		const rows = page.locator('.xterm-rows');
		await h.until(async () => /[#$] $/m.test((await rows.innerText()).trimEnd() + ' '), { message: 'a prompt' });
		await page.getByRole('textbox', { name: 'Terminal input' }).focus();
		// In a child shell: busybox runs a plain sleep inside the shell itself.
		await page.keyboard.type("sh -c 'sleep 4242; true'\n");
		await h.until(async () => running(), { message: 'sleep running in the container' });
		// Esc twice gives the keys back to Kubri, a third closes the inspector and the shell.
		await page.keyboard.press('Escape');
		await page.keyboard.press('Escape');
		const input = page.getByRole('textbox', { name: 'Terminal input' });
		await h.until(async () => !(await input.evaluate((el) => el === document.activeElement)), { message: 'the keys back' });
		await page.keyboard.press('Escape');
		await page.getByRole('complementary', { name: 'Pod inspector' }).waitFor({ state: 'hidden' });
		await h.until(async () => !running(), { message: 'the closed shell to end in the container' });
	});
});

describe('port-forward', () => {
	check('forwards a local port to a pod and stops again', async () => {
		const port = await h.freePort();
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: 'web' });
		await page.keyboard.press('Shift+F');
		const dialog = page.getByRole('dialog', { name: 'Port-forward' });
		await dialog.getByRole('spinbutton', { name: 'Local port' }).fill(String(port));
		await dialog.getByRole('button', { name: 'Start', exact: true }).click();
		await dialog.waitFor({ state: 'hidden' });

		let body = '';
		await h.until(
			async () => {
				body = await (await fetch(`http://127.0.0.1:${port}/`)).text();
				return body.includes('hello from web');
			},
			{ message: `an answer on port ${port}` }
		);

		// A client that stops sending after its request still gets the whole answer.
		const halfClosed = await new Promise((resolve, reject) => {
			const socket = net.connect(port, '127.0.0.1', () => socket.end('GET / HTTP/1.0\r\n\r\n'));
			let answer = '';
			socket.setEncoding('utf8');
			socket.on('data', (chunk) => (answer += chunk));
			socket.on('end', () => resolve(answer));
			socket.on('error', reject);
			socket.setTimeout(20_000, () => reject(new Error('no answer after a half-close')));
		});
		assert.match(halfClosed, /hello from web/);

		await h.command(page, 'pf');
		const row = page.getByRole('row', { name: new RegExp(`\\b${port}\\b`) });
		await row.waitFor();
		await row.click();
		await page.keyboard.press('Control+d');
		const stop = page.getByRole('dialog').filter({ hasText: String(port) });
		if (await stop.isVisible().catch(() => false)) await stop.getByRole('button', { name: /^(Stop|Delete)$/ }).click();
		await row.waitFor({ state: 'detached' });
		await h.until(
			async () => {
				try {
					await fetch(`http://127.0.0.1:${port}/`, { signal: AbortSignal.timeout(2000) });
					return false;
				} catch {
					return true;
				}
			},
			{ message: `port ${port} to close` }
		);
	});
});

describe('favorite port-forwards', () => {
	check('keeps a forward as a favorite that starts and stops with one click', async () => {
		const port = await h.freePort();
		const answers = async () => {
			try {
				return (await (await fetch(`http://127.0.0.1:${port}/`, { signal: AbortSignal.timeout(2000) })).text()).includes('hello from web');
			} catch {
				return false;
			}
		};
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods', filter: 'web' });
		await page.keyboard.press('Shift+F');
		const dialog = page.getByRole('dialog', { name: 'Port-forward' });
		await dialog.getByRole('spinbutton', { name: 'Local port' }).fill(String(port));
		await dialog.getByRole('checkbox', { name: 'Keep as a favorite' }).check();
		await dialog.getByRole('button', { name: 'Start', exact: true }).click();
		await dialog.waitFor({ state: 'hidden' });
		await h.until(answers, { message: `an answer on port ${port}` });

		const favorite = page.getByRole('navigation', { name: 'Cluster navigation' }).getByRole('button', { name: new RegExp(`:${port}$`) });
		await favorite.waitFor();
		await h.until(async () => (await favorite.getAttribute('aria-pressed')) === 'true', { message: 'the favorite shown running' });
		await favorite.click();
		await h.until(async () => (await favorite.getAttribute('aria-pressed')) === 'false', { message: 'the favorite shown stopped' });
		await h.until(async () => !(await answers()), { message: `port ${port} to close` });
		await favorite.click();
		await h.until(answers, { message: `port ${port} to answer again` });

		await h.command(page, 'pf');
		const kept = page.getByRole('listitem').filter({ hasText: `:${port}` });
		await kept.getByRole('button', { name: 'Stop', exact: true }).click();
		await kept.getByRole('button', { name: 'Start', exact: true }).waitFor();
		await kept.getByRole('button', { name: 'Remove', exact: true }).click();
		await favorite.waitFor({ state: 'detached' });
	});
});

describe('changes', () => {
	check('edits YAML with a reviewed diff', async () => {
		h.kubectl('patch', 'configmap', 'settings', '-n', 'e2e', '--type=merge', '-p', '{"data":{"mode":"test"}}');
		await h.openList(page, server.url, { resource: 'configmaps.v1', heading: 'ConfigMaps', filter: 'settings' });
		await page.keyboard.press('e');
		const editor = page.locator('.cm-content[contenteditable=true]');
		await editor.waitFor();
		await editor.fill((await editor.innerText()).replace('mode: test', 'mode: edited'));
		await page.keyboard.press('Control+s');
		const changes = page.getByRole('region', { name: 'Changes' });
		await changes.waitFor();
		assert.match(await changes.innerText(), /mode: edited/);
		await page.getByRole('button', { name: 'Save', exact: true }).click();
		await h.until(
			() => h.kubectl('get', 'configmap', 'settings', '-n', 'e2e', '-o', 'jsonpath={.data.mode}') === 'edited',
			{ message: 'the change in the cluster' }
		);
	});

	check('scales a deployment', async () => {
		h.kubectl('create', 'deployment', 'scaled', '-n', 'e2e', '--image=busybox:1.36', '--replicas=0', '--', 'sleep', '1000000');
		try {
			await h.openList(page, server.url, { resource: 'deployments.v1.apps', heading: 'Deployments', filter: 'scaled' });
			await page.keyboard.press('s');
			const dialog = page.getByRole('dialog', { name: 'Scale Deployment' });
			await dialog.getByRole('spinbutton', { name: 'Replicas' }).fill('2');
			await dialog.getByRole('button', { name: 'Scale', exact: true }).click();
			await h.until(
				() => h.kubectl('get', 'deployment', 'scaled', '-n', 'e2e', '-o', 'jsonpath={.spec.replicas}') === '2',
				{ message: 'two replicas' }
			);
		} finally {
			h.kubectl('delete', 'deployment', 'scaled', '-n', 'e2e', '--ignore-not-found', '--wait=false');
		}
	});

	check('restarts a deployment', async () => {
		h.kubectl('create', 'deployment', 'restarted', '-n', 'e2e', '--image=busybox:1.36', '--replicas=0', '--', 'sleep', '1000000');
		try {
			await h.openList(page, server.url, { resource: 'deployments.v1.apps', heading: 'Deployments', filter: 'restarted' });
			await page.keyboard.press('r');
			const dialog = page.getByRole('dialog', { name: /^Restart/ });
			await dialog.getByRole('button', { name: 'Restart', exact: true }).click();
			const annotation = '{.spec.template.metadata.annotations.kubectl\\.kubernetes\\.io/restartedAt}';
			await h.until(
				() => h.kubectl('get', 'deployment', 'restarted', '-n', 'e2e', '-o', `jsonpath=${annotation}`) !== '',
				{ message: 'the restartedAt annotation' }
			);
		} finally {
			h.kubectl('delete', 'deployment', 'restarted', '-n', 'e2e', '--ignore-not-found', '--wait=false');
		}
	});

	check('sets requests and limits with the room on the nodes', async () => {
		h.kubectl('create', 'deployment', 'sized', '-n', 'e2e', '--image=busybox:1.36', '--replicas=0', '--', 'sleep', '1000000');
		try {
			await h.openList(page, server.url, { resource: 'deployments.v1.apps', heading: 'Deployments', filter: 'sized' });
			await page.keyboard.press('Shift+Q');
			const dialog = page.getByRole('dialog', { name: 'Requests & limits' });
			const request = dialog.getByRole('combobox', { name: 'busybox: CPU request' });
			await request.waitFor();
			// Focused, empty: the arrow key starts at a small size, then steps up.
			await page.keyboard.press('ArrowUp');
			await page.keyboard.press('ArrowUp');
			assert.equal(await request.inputValue(), '150m');
			await h.until(async () => /requested/.test(await dialog.getByRole('region', { name: 'Room on the nodes' }).innerText()), {
				message: 'the room on the nodes'
			});
			await dialog.getByRole('button', { name: 'Review changes' }).click();
			await dialog.getByRole('button', { name: 'Set', exact: true }).click();
			await h.until(
				() => h.kubectl('get', 'deployment', 'sized', '-n', 'e2e', '-o', 'jsonpath={.spec.template.spec.containers[0].resources.requests.cpu}') === '150m',
				{ message: 'the new CPU request' }
			);
		} finally {
			h.kubectl('delete', 'deployment', 'sized', '-n', 'e2e', '--ignore-not-found', '--wait=false');
		}
	});

	check('deletes after confirming', async () => {
		h.kubectl('create', 'configmap', 'doomed', '-n', 'e2e', '--from-literal=a=b');
		await h.openList(page, server.url, { resource: 'configmaps.v1', heading: 'ConfigMaps', filter: 'doomed' });
		await page.keyboard.press('Control+d');
		const dialog = page.getByRole('dialog', { name: 'Delete ConfigMap' });
		assert.match(await dialog.innerText(), /e2e\/doomed/);
		await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
		await h.until(() => !h.kubectl('get', 'configmaps', '-n', 'e2e', '-o', 'name').includes('configmap/doomed'), {
			message: 'the ConfigMap to go'
		});
		await page.getByRole('button', { name: 'doomed', exact: true }).waitFor({ state: 'detached' });
	});
});

describe('read-only', () => {
	let readOnly;
	let roPage;
	before(async () => {
		readOnly = await h.startServer({ args: ['--read-only'] });
		roPage = await h.openPage(readOnly.url);
	});
	after(async () => {
		await roPage?.context().close();
		await readOnly?.stop();
	});

	check('explains instead of deleting', async () => {
		await h.openList(roPage, readOnly.url, { resource: 'configmaps.v1', heading: 'ConfigMaps', filter: 'settings' });
		await roPage.keyboard.press('Control+d');
		await roPage.getByText(/Delete is not available/).waitFor();
		assert.equal(await roPage.getByRole('dialog', { name: 'Delete ConfigMap' }).count(), 0);
		const target = { resource: { group: '', version: 'v1', plural: 'configmaps' }, namespace: 'e2e', name: 'settings' };
		const response = await fetch(`${readOnly.url}/api/call`, {
			method: 'POST',
			headers: { 'content-type': 'application/json', origin: readOnly.url },
			body: JSON.stringify({ call: 'deleteObject', target, options: null })
		});
		assert.equal(response.status, 403, 'the server refuses a delete');
		assert.match(h.kubectl('get', 'configmap', 'settings', '-n', 'e2e', '-o', 'name'), /settings/);
	});
});

describe('snapshots', () => {
	check('saves a namespace and opens it without the cluster', async () => {
		await h.openList(page, server.url, { resource: 'pods.v1', heading: 'Pods' });
		await h.command(page, 'snapshot e2e');
		const dialog = page.getByRole('dialog', { name: 'Snapshots' });
		const [download] = await Promise.all([
			page.waitForEvent('download', { timeout: 60_000 }),
			dialog.getByRole('button', { name: 'Download', exact: true }).click()
		]);
		const file = path.join(h.work, 'e2e.kubri');
		await download.saveAs(file);
		assert.ok(fs.statSync(file).size > 1000, 'the snapshot holds something');

		const offline = await h.startServer({ args: ['--snapshot', file], env: { KUBECONFIG: path.join(h.work, 'missing') } });
		const snapPage = await h.openPage(offline.url);
		try {
			await snapPage.getByText(/snapshot/i).first().waitFor();
			await h.command(snapPage, 'pods');
			await snapPage.getByRole('button', { name: webPod(), exact: true }).waitFor();
		} finally {
			await snapPage.context().close();
			await offline.stop();
		}
	});
});

describe('MCP', () => {
	check('answers an agent over stdio', async () => {
		const child = spawn(process.env.KUBRI_BIN || path.resolve(__dirname, '../../target/release/kubri-server'), ['--mcp'], {
			env: { ...process.env, KUBECONFIG: h.kubeconfig },
			stdio: ['pipe', 'pipe', 'inherit']
		});
		const replies = new Map();
		let buffer = '';
		child.stdout.on('data', (d) => {
			buffer += d;
			let nl;
			while ((nl = buffer.indexOf('\n')) >= 0) {
				const line = buffer.slice(0, nl).trim();
				buffer = buffer.slice(nl + 1);
				if (!line) continue;
				const message = JSON.parse(line);
				if (message.id !== undefined) replies.set(message.id, message);
			}
		});
		const send = (message) => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
		const reply = async (id) => {
			await h.until(() => replies.has(id), { message: `MCP reply ${id}` });
			return replies.get(id);
		};
		try {
			send({ id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } });
			assert.ok((await reply(1)).result, 'initialize succeeds');
			send({ method: 'notifications/initialized' });
			send({ id: 2, method: 'tools/list' });
			const tools = (await reply(2)).result.tools.map((t) => t.name);
			assert.ok(tools.includes('list_objects'), `tools: ${tools.join(', ')}`);
			send({
				id: 3,
				method: 'tools/call',
				params: { name: 'list_objects', arguments: { context: 'e2e-one', resource: 'pods', namespace: 'e2e' } }
			});
			const result = (await reply(3)).result;
			const text = result.content.map((c) => c.text ?? '').join('\n');
			assert.ok(!result.isError, text);
			assert.match(text, /db-0/);
		} finally {
			child.kill('SIGTERM');
		}
	});
});

describe('gentle on the cluster', () => {
	check('the Clusters page checks each context once, not again and again', async () => {
		const asked = new Map();
		const seen = (request) => {
			if (request.method() !== 'POST' || !request.url().endsWith('/api/call')) return;
			const body = request.postDataJSON();
			if (body?.call === 'fleetCluster') asked.set(body.context, (asked.get(body.context) ?? 0) + 1);
		};
		page.on('request', seen);
		try {
			await page.goto('about:blank');
			await page.goto(`${server.url}/#/e2e-one/fleet`);
			await page.getByRole('heading', { name: 'Clusters', level: 1, exact: true }).waitFor();
			await new Promise((resolve) => setTimeout(resolve, 8000));
		} finally {
			page.off('request', seen);
		}
		// One round; the next is due in minutes.
		assert.ok(asked.get('e2e-one') >= 1, 'checks e2e-one');
		for (const [context, times] of asked) assert.equal(times, 1, `checked ${context} ${times} times in 8 seconds`);
	});
});

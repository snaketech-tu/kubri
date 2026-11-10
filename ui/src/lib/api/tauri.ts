import { Channel, invoke } from '@tauri-apps/api/core';
import { writeQueue } from './queue';
import { callBackend } from './rpc';
import type { Backend, LogEvent, TableEvent, TerminalEvent, TerminalSession, UpdateProgress } from './types';

/** Opens a terminal with `command`; all terminals share the write, resize and close commands. */
function terminalChannel(
	command: string,
	args: Record<string, unknown>,
	onEvent: (event: TerminalEvent) => void
): TerminalSession {
	let id: number | null = null;
	let state: 'opening' | 'open' | 'exited' | 'closed' = 'opening';
	const input = writeQueue(
		(data) => invoke('terminal_write', { id, data }),
		() => {}
	);
	// Teardown is best-effort: the process or transport may have already gone.
	const closeNative = () => invoke('terminal_close', { id }).catch(() => {});
	// Output arrives as raw bytes, control messages as JSON.
	const channel = new Channel<ArrayBuffer | Exclude<TerminalEvent, { type: 'data' }>>();
	channel.onmessage = (msg) => {
		if (state === 'closed' || state === 'exited') return;
		if (!(msg instanceof ArrayBuffer) && msg.type === 'exit') {
			state = 'exited';
			input.close();
		}
		onEvent(msg instanceof ArrayBuffer ? { type: 'data', data: new Uint8Array(msg) } : msg);
	};
	invoke<number>(command, { ...args, onEvent: channel }).then(
		(opened) => {
			id = opened;
			if (state === 'closed') void closeNative();
			else if (state === 'opening') {
				state = 'open';
				input.start();
			}
		},
		(err) => {
			input.close();
			if (state === 'closed' || state === 'exited') return;
			state = 'exited';
			onEvent({ type: 'exit', message: String(err) });
		}
	);
	return {
		write: (data) => input.write(data),
		resize(cols, rows) {
			if (state === 'open') invoke('terminal_resize', { id, cols, rows }).catch(() => {});
		},
		close() {
			if (state === 'closed') return;
			state = 'closed';
			input.close();
			if (id !== null) void closeNative();
		}
	};
}

/** Talks to the Rust side of the desktop app through Tauri commands. */
export const tauriBackend: Backend = {
	...callBackend((request) => invoke('call', { request })),
	chooseKubeconfigFiles: () => invoke('choose_kubeconfig_files'),
	addKubeconfigFiles: (paths) => invoke('add_kubeconfig_files', { paths }),
	removeKubeconfigFile: (path) => invoke('remove_kubeconfig_file', { path }),
	takeSnapshot: (namespace) => invoke('take_snapshot', { namespace }),
	openSnapshot: async (file) =>
		invoke('open_snapshot', new Uint8Array(await file.arrayBuffer()), { headers: { 'x-name': encodeURIComponent(file.name) } }),
	agentConnection: () => invoke('agents_connection'),
	newAgentToken: () => invoke('agents_new_token'),
	agentRequests: () => invoke('agents_pending'),
	answerAgent: (id, allow) => invoke('agents_answer', { id, allow }),
	agentCalls: () => invoke('agents_calls'),
	explainKey: () => invoke('explain_key'),
	setExplainKey: (key) => invoke('set_explain_key', { key }),
	explainPrompt: (subject) => invoke('explain_prompt', { subject }),
	explain: (text) => invoke('explain', { text }),

	watchTable(resource, namespace, scope, onEvent) {
		const channel = new Channel<TableEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('watch_table', { resource, namespace, scope, onEvent: channel }).catch((err) => {
			onEvent({ type: 'error', message: String(err) });
			return null;
		});
		return () => {
			id.then((id) => id !== null && invoke('unwatch', { id }));
		};
	},

	streamLogs(target, options, onEvent) {
		const channel = new Channel<LogEvent>();
		channel.onmessage = onEvent;
		const id = invoke<number>('logs', { target, options, onEvent: channel }).catch((err) => {
			onEvent({ type: 'error', source: null, message: String(err) });
			onEvent({ type: 'end' });
			return null;
		});
		return () => {
			id.then((id) => id !== null && invoke('unwatch', { id }));
		};
	},

	openTerminal: (target, options, onEvent) => terminalChannel('terminal_open', { target, options }, onEvent),

	checkUpdate: () => invoke('update_check'),
	installUpdate(onProgress) {
		const channel = new Channel<UpdateProgress>();
		channel.onmessage = onProgress;
		return invoke('update_install', { onProgress: channel });
	},
	openPlugin: (run, onEvent) => terminalChannel('plugin_open', { run }, onEvent),
	createLocalCluster: (request, onEvent) => terminalChannel('local_cluster_create', { request }, onEvent),
	openLocalTerminal: (options, onEvent) => terminalChannel('local_terminal_open', { options }, onEvent),
	desktop: true,
	openUrl: (url) => invoke('open_url', { url }),
	saveText: (name, text) => invoke('save_text', { name, text })
};

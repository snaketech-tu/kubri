import { describe, expect, it } from 'vitest';
import type { Alias, Plugin, ResourceInfo } from './api';
import { resourceArgs, resourceByPath, suggest } from './commands';
import {
	expandVariables,
	focusVariables,
	inputError,
	inputVariables,
	kubriUses,
	matchesShortcut,
	parseShortcut,
	pluginApplies,
	shortcutLabel,
	skinTheme
} from './k9s';

function resource(kind: string, plural: string, group = '', shortNames: string[] = []): ResourceInfo {
	return {
		group,
		version: 'v1',
		plural,
		kind,
		singular: kind.toLowerCase(),
		shortNames,
		namespaced: true,
		verbs: ['list', 'watch'],
		subresources: []
	};
}

const pods = resource('Pod', 'pods', '', ['po']);
const deployments = resource('Deployment', 'deployments', 'apps', ['deploy']);
const bindings = resource('ClusterRoleBinding', 'clusterrolebindings', 'rbac.authorization.k8s.io');
const resources = [pods, deployments, bindings];

function key(init: Partial<KeyboardEvent> & { key: string; code?: string }): KeyboardEvent {
	return { ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, code: '', ...init } as KeyboardEvent;
}

function plugin(scopes: string[], extra: Partial<Plugin> = {}): Plugin {
	return {
		name: 'p',
		shortCut: 'Ctrl-L',
		description: '',
		scopes,
		command: 'kubectl',
		args: [],
		background: false,
		confirm: false,
		dangerous: false,
		override: false,
		inputs: [],
		...extra
	};
}

describe('k9s shortcuts', () => {
	it('parses the forms k9s uses', () => {
		expect(parseShortcut('Ctrl-L')).toEqual({ key: 'l', ctrl: true, shift: false, alt: false });
		expect(parseShortcut('Shift-D')).toEqual({ key: 'd', ctrl: false, shift: true, alt: false });
		expect(parseShortcut('a')).toEqual({ key: 'a', ctrl: false, shift: false, alt: false });
		expect(parseShortcut('D')?.shift).toBe(true);
		expect(parseShortcut('Shift-0')).toEqual({ key: '0', ctrl: false, shift: true, alt: false });
		expect(parseShortcut('alt-k')?.alt).toBe(true);
		expect(parseShortcut('F5')?.key).toBe('F5');
		expect(parseShortcut('Hyper-X')).toBeNull();
		expect(parseShortcut('Enter')).toBeNull();
		expect(parseShortcut('')).toBeNull();
	});

	it('matches key presses, on any keyboard layout', () => {
		const ctrlL = parseShortcut('Ctrl-L')!;
		expect(matchesShortcut(key({ key: 'l', ctrlKey: true }), ctrlL)).toBe(true);
		expect(matchesShortcut(key({ key: 'l', metaKey: true }), ctrlL)).toBe(true);
		expect(matchesShortcut(key({ key: 'l' }), ctrlL)).toBe(false);
		expect(matchesShortcut(key({ key: 'L', ctrlKey: true, shiftKey: true }), ctrlL)).toBe(false);
		// Shift-0 types ')' on a US keyboard and '=' on a German one.
		const shift0 = parseShortcut('Shift-0')!;
		expect(matchesShortcut(key({ key: '=', code: 'Digit0', shiftKey: true }), shift0)).toBe(true);
		expect(matchesShortcut(key({ key: '0', code: 'Digit0' }), shift0)).toBe(false);
		// Alt on a Mac types other characters; the key's position decides then.
		expect(matchesShortcut(key({ key: '˚', code: 'KeyK', altKey: true }), parseShortcut('Alt-K')!)).toBe(true);
		// On a German keyboard y and z swap places; the letter typed counts.
		expect(matchesShortcut(key({ key: 'z', code: 'KeyY' }), parseShortcut('z')!)).toBe(true);
	});

	it('labels keys like the footer and knows the ones Kubri uses', () => {
		expect(shortcutLabel(parseShortcut('Ctrl-L')!)).toBe('^l');
		expect(shortcutLabel(parseShortcut('Shift-F')!)).toBe('⇧f');
		expect(kubriUses(parseShortcut('Shift-F')!)).toBe(true);
		expect(kubriUses(parseShortcut('Ctrl-D')!)).toBe(true);
		expect(kubriUses(parseShortcut('Ctrl-L')!)).toBe(true);
		expect(kubriUses(parseShortcut('Ctrl-X')!)).toBe(false);
		expect(kubriUses(parseShortcut('Shift-0')!)).toBe(false);
	});
});

describe('k9s plugins', () => {
	it('offers plugins in the views their scopes name', () => {
		const inPods = { kind: 'resource', resource: pods } as const;
		const inDeployments = { kind: 'resource', resource: deployments } as const;
		expect(pluginApplies(plugin(['po']), inPods)).toBe(true);
		expect(pluginApplies(plugin(['containers']), inPods)).toBe(true);
		expect(pluginApplies(plugin(['containers']), inDeployments)).toBe(false);
		expect(pluginApplies(plugin(['apps/v1/deployments']), inDeployments)).toBe(true);
		expect(pluginApplies(plugin([' Deploy ']), inDeployments)).toBe(true);
		expect(pluginApplies(plugin(['helm']), { kind: 'helm' })).toBe(true);
		expect(pluginApplies(plugin(['po']), { kind: 'helm' })).toBe(false);
		expect(pluginApplies(plugin(['all']), { kind: 'other' })).toBe(true);
		expect(pluginApplies(plugin(['po']), { kind: 'other' })).toBe(false);
	});

	it('expands variables like the plugin runner', () => {
		const vars = { NAME: 'web-1', NAMESPACE: 'shop' };
		const columns = [
			{ name: 'Name', value: 'web-1' },
			{ name: '%CPU/L', value: '12' }
		];
		const x = (text: string) => expandVariables(text, vars, columns);
		expect(x('$NAME -n $NAMESPACE')).toBe('web-1 -n shop');
		expect(x('${NAME}-debug')).toBe('web-1-debug');
		expect(x('$COL-NAME/$COL-0')).toBe('web-1/web-1');
		expect(x('${COL-%CPU/L}%')).toBe('12%');
		expect(x('$COL-MISSING.')).toBe('.');
		expect(x('read line; echo $line')).toBe('read line; echo $line');
		expect(x('costs $ 5 and ${oops')).toBe('costs $ 5 and ${oops');
	});

	it('knows the variables of the row under the cursor', () => {
		const context = { name: 'prod', cluster: 'eu-1', user: 'admin', namespace: null, isDefault: true, added: false };
		const vars = focusVariables(
			{
				resource: pods,
				target: { resource: { group: '', version: 'v1', plural: 'pods' }, namespace: 'shop', name: 'web-1' },
				release: null,
				columns: [],
				filter: 'web'
			},
			context
		);
		expect(vars).toMatchObject({ NAME: 'web-1', POD: 'web-1', NAMESPACE: 'shop', RESOURCE_NAME: 'pods', CONTEXT: 'prod', FILTER: 'web' });
		const helm = focusVariables(
			{ resource: null, target: null, release: { namespace: 'shop', name: 'cart' }, columns: [], filter: '' },
			null
		);
		expect(helm).toMatchObject({ NAME: 'cart', NAMESPACE: 'shop', POD: '', RESOURCE_NAME: 'helm' });
	});
});

describe('k9s aliases in the palette', () => {
	const aliases: Alias[] = [
		{ name: 'pp', command: 'v1/pods' },
		{ name: 'crb', command: 'rbac.authorization.k8s.io/v1/clusterrolebindings' },
		{ name: 'fred', command: 'pod fred app=blee' },
		{ name: 'dp', command: 'deploy' },
		{ name: 'ddp', command: 'dp kube-system' },
		{ name: 'loop', command: 'loop' }
	];
	const config = { aliases, skins: ['dracula', 'nord'] };
	const run = (input: string) => suggest(input, resources, [], [], config)[0]?.action;

	it('resolves resource paths', () => {
		expect(resourceByPath('v1/pods', resources)).toBe(pods);
		expect(resourceByPath('apps/v1/deployments', resources)).toBe(deployments);
		// Another served version still finds the resource.
		expect(resourceByPath('apps/v1beta1/deployments', resources)).toBe(deployments);
		expect(resourceByPath('v1/', resources)).toBeUndefined();
		expect(run('apps/v1/deployments shop')).toMatchObject({ type: 'resource', resource: deployments, namespace: 'shop' });
	});

	it('reads namespaces, labels and contexts after a resource', () => {
		expect(resourceArgs(['kube-system'])).toEqual({ namespace: 'kube-system' });
		expect(resourceArgs(['app=web', 'tier=front'])).toEqual({ filter: '-l app=web,tier=front' });
		expect(resourceArgs(['-l', 'app=web', 'shop', '@prod'])).toEqual({ namespace: 'shop', filter: '-l app=web', context: 'prod' });
		expect(run('po shop app=web')).toMatchObject({ type: 'resource', resource: pods, namespace: 'shop', filter: '-l app=web' });
	});

	it('expands aliases, also into other aliases', () => {
		expect(run('pp')).toMatchObject({ type: 'resource', resource: pods });
		expect(run('pp shop')).toMatchObject({ type: 'resource', resource: pods, namespace: 'shop' });
		expect(run('crb')).toMatchObject({ type: 'resource', resource: bindings });
		expect(run('fred')).toMatchObject({ type: 'resource', resource: pods, namespace: 'fred', filter: '-l app=blee' });
		expect(run('ddp')).toMatchObject({ type: 'resource', resource: deployments, namespace: 'kube-system' });
		expect(suggest('pp', resources, [], [], config)[0]).toMatchObject({ label: 'pp', detail: 'v1/pods · alias' });
		// An alias for itself ends instead of looping.
		expect(() => suggest('loop', resources, [], [], config)).not.toThrow();
	});

	it('offers aliases while typed', () => {
		expect(suggest('cr', resources, [], [], config).map((s) => s.label)).toContain('crb');
	});

	it('opens the config page and switches skins', () => {
		expect(run('plugins')).toEqual({ type: 'config', section: 'plugins' });
		expect(run('k9s')).toEqual({ type: 'config' });
		expect(run('skin dra')).toEqual({ type: 'skin', name: 'dracula' });
		expect(run('skin kubri')).toEqual({ type: 'skin', name: null });
		expect(suggest('skin', resources, [], [], config).map((s) => s.label)).toEqual(['skin dracula', 'skin nord', 'skin kubri']);
	});
});

describe('k9s skins', () => {
	const dracula = {
		k9s: {
			body: { fgColor: '#f8f8f2', bgColor: '#282a36', logoColor: '#bd93f9' },
			frame: { status: { addColor: '#50fa7b', highlightcolor: '#ffb86c', errorColor: '#ff5555' } },
			views: { yaml: { keyColor: '#ff79c6', valueColor: '#f8f8f2' } }
		}
	};

	it('turns skin colors into Kubri colors', () => {
		expect(skinTheme(dracula)).toEqual({
			vars: {
				'--bg': '#282a36',
				'--text': '#f8f8f2',
				'--accent': '#bd93f9',
				'--on-accent': '#10131a',
				'--ok': '#50fa7b',
				'--warn': '#ffb86c',
				'--bad': '#ff5555',
				'--syntax-key': '#ff79c6',
				'--syntax-string': '#f8f8f2'
			},
			scheme: 'dark'
		});
	});

	it('keeps Kubri colors where a skin says default', () => {
		const transparent = { k9s: { body: { fgColor: 'default', bgColor: 'default', logoColor: '#fff' } } };
		expect(skinTheme(transparent)).toEqual({ vars: { '--accent': '#ffffff', '--on-accent': '#10131a' }, scheme: 'dark' });
		expect(skinTheme({ k9s: { body: { bgColor: '#fdf6e3', fgColor: '#657b83' } } })?.scheme).toBe('light');
		expect(skinTheme({ k9s: { body: { fgColor: 'default' } } })).toBeNull();
		expect(skinTheme('not a skin')).toBeNull();
		// Without a resolver for names, only hex colors count.
		expect(skinTheme({ k9s: { body: { bgColor: 'black' } } })).toBeNull();
		expect(skinTheme({ k9s: { body: { bgColor: 'black' } } }, (c) => (c === 'black' ? '#000000' : null))?.vars).toEqual({
			'--bg': '#000000'
		});
	});
});

describe('plugin inputs', () => {
	const tail = plugin(['pods'], {
		inputs: [
			{ name: 'lines', label: 'Lines', type: 'number', required: true, default: '50', options: [] },
			{ name: 'level', label: '', type: 'dropdown', required: false, default: '', options: ['info', 'debug'] },
			{ name: 'follow', label: 'Follow', type: 'bool', required: false, default: 'false', options: [] }
		]
	});

	it('become variables, with defaults for those not given', () => {
		expect(inputVariables(tail, { level: 'debug' })).toEqual({
			INPUT_LINES: '50',
			INPUT_LEVEL: 'debug',
			INPUT_FOLLOW: 'false'
		});
	});

	it('are checked as the plugin runner checks them', () => {
		expect(inputError(tail, {})).toBeNull();
		expect(inputError(tail, { lines: ' ' })).toBe('Lines is required');
		expect(inputError(tail, { lines: 'ten' })).toBe('Lines must be a number');
		expect(inputError(tail, { lines: '1.5e3' })).toBeNull();
		expect(inputError(tail, { level: 'trace' })).toBe('level must be one of info, debug');
		expect(inputError(tail, { follow: 'yes' })).toBe('Follow must be true or false');
	});
});

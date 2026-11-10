import { backend, type ResourceInfo } from '../api';
import type { Action } from '../commands';
import { feedbackLink } from '../feedback';
import { errorMessage } from '../format';
import { t } from '../i18n';
import type { AppState } from '../state.svelte';
import { findResource } from './model';

/** Runs a palette command or hotkey. */
export async function run(app: AppState, action: Action) {
	switch (action.type) {
		case 'resource': {
			let resource: ResourceInfo | undefined = action.resource;
			if (action.context && action.context !== app.cluster?.context) {
				await app.switchContext(action.context);
				if (app.cluster?.context !== action.context) return;
				resource = findResource(app.resources, action.resource);
				if (!resource) {
					app.toast(t('{context} has no {kind}', { context: action.context, kind: action.resource.plural }), 'bad');
					return;
				}
			}
			app.open(resource, action.namespace, action.filter);
			break;
		}
		case 'context': {
			const name = action.name === '-' ? app.previousContext : action.name;
			if (!name) app.toast(t('There is no previous context yet'), 'bad');
			else if (name !== app.cluster?.context) await app.switchContext(name);
			break;
		}
		case 'namespace':
			app.namespace = action.name;
			break;
		case 'pulses':
			app.showPulses(action.namespace);
			break;
		case 'xray':
			app.showXray(action.resource, action.namespace);
			break;
		case 'rbac':
			app.showRbac(action.namespace);
			break;
		case 'helm':
			app.showHelm(action.namespace);
			break;
		case 'new':
			app.showCreate(action.resource, action.params);
			break;
		case 'sanitizer':
			app.showSanitizer(action.namespace);
			break;
		case 'benchmarks':
			app.showBenchmarks();
			break;
		case 'integrations':
			app.showIntegrations();
			break;
		case 'rightsizing':
			app.showRightSizing();
			break;
		case 'tools':
			app.showTools();
			break;
		case 'kinds':
			app.showKinds(action.filter);
			break;
		case 'fleet':
			app.showFleet();
			break;
		case 'terminal':
			app.openLocalTerminal(action.another);
			break;
		case 'search':
			app.showSearch(action.query);
			break;
		case 'graph':
			app.showGraph(action.namespace);
			break;
		case 'changes':
			app.showChanges(action.filter);
			break;
		case 'snapshot':
			app.showSnapshot(action.namespace);
			break;
		case 'kubeconfig':
			app.kubeconfigFilesOpen = true;
			break;
		case 'object':
			await app.openObject(action.target, action.kind);
			break;
		case 'portforwards':
			app.showPortForwards();
			break;
		case 'config':
			app.showConfig(action.section);
			break;
		case 'skin':
			app.appearance.setSkin(action.name);
			break;
		case 'help':
			app.helpOpen = true;
			break;
		case 'feedback':
			backend.openUrl(feedbackLink(action.kind, backend.desktop)).catch((err) => app.toast(errorMessage(err), 'bad'));
			break;
		case 'call':
			action.run();
			break;
	}
}

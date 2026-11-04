import { describe, expect, it } from 'vitest';
import { fitsText, placeholders, templateFromUrl, titleFromUrl } from './grafana';

describe('templateFromUrl', () => {
	it('turns the variables Kubri knows into placeholders', () => {
		expect(
			templateFromUrl(
				'https://grafana.kunde.de/d/abc/pods?orgId=1&var-datasource=prom&var-namespace=shop&var-pod=web-1&from=1730000000000&to=1730003600000'
			)
		).toBe('https://grafana.kunde.de/d/abc/pods?orgId=1&var-datasource=prom&var-namespace={namespace}&var-pod={pod}');
	});

	it('keeps relative time ranges and maps workload types to the kind', () => {
		expect(templateFromUrl('https://g.example.com/d/wl/x?var-type=deployment&var-workload=web&from=now-6h&to=now')).toBe(
			'https://g.example.com/d/wl/x?var-type={kind}&var-workload={workload}&from=now-6h&to=now'
		);
	});

	it('leaves variables that are also Prometheus labels for other things', () => {
		expect(templateFromUrl('https://g/d/x/node-exporter?var-job=node&var-node=n1&var-type=a')).toBe(
			'https://g/d/x/node-exporter?var-job=node&var-node={node}&var-type=a'
		);
	});

	it('leaves templates, paths and text alone', () => {
		expect(templateFromUrl(' https://g/d/a?var-pod={pod} ')).toBe('https://g/d/a?var-pod={pod}');
		expect(templateFromUrl('/d/a?var-pod=web')).toBe('/d/a?var-pod=web');
		expect(templateFromUrl('https://g.example.com/d/abc/pods')).toBe('https://g.example.com/d/abc/pods');
	});
});

describe('titleFromUrl', () => {
	it('names a link after the dashboard', () => {
		expect(titleFromUrl('https://g/d/abc/kubernetes-compute-resources-pod?var-pod=x')).toBe('Kubernetes compute resources pod');
		expect(titleFromUrl('https://g/explore')).toBe('');
		expect(titleFromUrl('not a url')).toBe('');
	});
});

describe('placeholders and fitsText', () => {
	it('says which objects a link is for', () => {
		expect(placeholders('a={namespace}&b={pod}&c={pod}')).toEqual(['namespace', 'pod']);
		expect(fitsText('x?var-namespace={namespace}&var-pod={pod}')).toBe('Shows for pods');
		expect(fitsText('x?var-node={node}')).toBe('Shows for nodes');
		expect(fitsText('x?i={ingress}')).toBe('Shows for ingresses');
		expect(fitsText('x?w={workload}&t={kind}')).toBe('Shows for workloads');
		expect(fitsText('x?ns={namespace}')).toBe('Shows for namespaces and everything in them');
		expect(fitsText('x?q={name}')).toBe('Shows for every object');
		expect(fitsText('x?p={pod}&n={node}')).toBe('Shows for no object: it needs both pod and node');
	});
});

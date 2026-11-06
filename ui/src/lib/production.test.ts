import { describe, expect, it } from 'vitest';
import { labelsMatch, nameMatches, productionRule, suggestions, tidyLabelRule, withMarked } from './production';

describe('production namespaces', () => {
	it('matches names against patterns as the core does', () => {
		const cases: [string, string, boolean][] = [
			['*-prod', 'shop-prod', true],
			['*-prod', 'shop-prod-2', false],
			['prod-*', 'prod-eu', true],
			['prod-*', 'prod-', true],
			['*prod*', 'payments-production', true],
			['a*b*c', 'abc', true],
			['a*b*c', 'acb', false],
			['ab*ba', 'aba', false],
			['shop', 'shop', true],
			['shop', 'shops', false],
			['*', 'anything', true]
		];
		for (const [pattern, name, yes] of cases) expect(nameMatches(pattern, name), `${pattern} ${name}`).toBe(yes);
	});

	it('matches labels against rules as the core does', () => {
		const labels = { env: 'production', team: 'shop' };
		const cases: [string, boolean][] = [
			['env=production', true],
			['env == production', true],
			['env=staging', false],
			['env!=staging', true],
			['tier!=web', true],
			['team', true],
			['!team', false],
			['!canary', true],
			['env=production,team=web', false],
			['env=production, team', true],
			['env in (production)', false],
			['', false]
		];
		for (const [rule, yes] of cases) expect(labelsMatch(rule, labels), rule).toBe(yes);
		expect(tidyLabelRule(' env = production , !canary ')).toBe('env=production,!canary');
		expect(tidyLabelRule('env in (prod)')).toBeNull();
	});

	it('says which rule marks a namespace, its own name first', () => {
		const settings = { productionNamespaces: ['*-prod', 'shop', 'shop-prod'], productionLabels: ['env=production'] };
		expect(productionRule(settings, 'shop-prod')).toEqual({ by: 'name', rule: 'shop-prod' });
		expect(productionRule(settings, 'web-prod')).toEqual({ by: 'pattern', rule: '*-prod' });
		expect(productionRule(settings, 'billing', { env: 'production' })).toEqual({ by: 'labels', rule: 'env=production' });
		// Without its labels, a label rule cannot tell; the core reads them before agents change anything.
		expect(productionRule(settings, 'billing')).toBeNull();
		expect(productionRule({ productionNamespaces: [] }, 'shop')).toBeNull();
	});

	it('marks and unmarks names, keeping the patterns', () => {
		const settings = { productionNamespaces: ['*-prod', 'shop'] };
		expect(withMarked(settings, ['billing', 'shop'], true)).toEqual(['*-prod', 'billing', 'shop']);
		expect(withMarked(settings, ['shop', 'web'], false)).toEqual(['*-prod']);
	});

	it('suggests namespaces that look like production, with the rule that finds them', () => {
		const settings = { productionNamespaces: ['shop'], productionLabels: [] };
		const found = suggestions(settings, [
			{ name: 'shop' },
			{ name: 'checkout-prod' },
			{ name: 'prod-eu' },
			{ name: 'production' },
			{ name: 'products' },
			{ name: 'billing', labels: { environment: 'Production' } },
			{ name: 'kube-prod' },
			{ name: 'staging', labels: { env: 'staging' } }
		]);
		expect(found).toEqual([
			{ name: 'billing', rule: 'environment=Production', by: 'labels' },
			{ name: 'checkout-prod', rule: '*-prod', by: 'pattern' },
			{ name: 'prod-eu', rule: 'prod-*', by: 'pattern' },
			{ name: 'production', rule: 'production', by: 'pattern' }
		]);
	});
});

import { describe, expect, it, vi } from 'vitest';
import { DraftGuard } from './navigation.svelte';

describe('unsaved YAML navigation', () => {
	it('allows clean navigation without prompting', () => {
		expect(new DraftGuard().block(vi.fn())).toBe(false);
	});
	it('holds navigation and preserves the draft on a failed save', async () => {
		const guard = new DraftGuard(),
			next = vi.fn();
		guard.dirty = true;
		guard.save = async () => false;
		expect(guard.block(next)).toBe(true);
		await guard.proceed('save');
		expect(next).not.toHaveBeenCalled();
		expect(guard.dirty).toBe(true);
		expect(guard.pending).toBe(next);
	});
	it('only continues after saving succeeds', async () => {
		const guard = new DraftGuard(),
			next = vi.fn();
		guard.dirty = true;
		guard.save = async () => true;
		guard.block(next);
		await guard.proceed('save');
		expect(next).toHaveBeenCalledOnce();
		expect(guard.dirty).toBe(false);
	});
	it('discards explicitly, then performs the original navigation once', async () => {
		const guard = new DraftGuard(),
			next = vi.fn(),
			discard = vi.fn();
		guard.dirty = true;
		guard.discard = discard;
		guard.block(next);
		await guard.proceed('discard');
		expect(discard).toHaveBeenCalledOnce();
		expect(next).toHaveBeenCalledOnce();
		expect(guard.pending).toBeNull();
	});
	it('cannot discard or navigate during a pending save', async () => {
		const guard = new DraftGuard(),
			next = vi.fn();
		guard.saving = true;
		guard.block(next);
		await guard.proceed('discard');
		expect(next).not.toHaveBeenCalled();
	});
});

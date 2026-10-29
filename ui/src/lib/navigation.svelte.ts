/** A single exit decision for every route out of an edited object. */
export class DraftGuard {
	dirty = $state(false);
	saving = $state(false);
	pending = $state<(() => unknown) | null>(null);
	save: (() => Promise<boolean>) | null = null;
	discard: (() => void) | null = null;
	block(action: () => unknown): boolean {
		if (!this.dirty && !this.saving) return false;
		this.pending = action;
		return true;
	}
	async proceed(choice: 'save' | 'discard') {
		if (this.saving) return;
		if (choice === 'save' && !(await this.save?.())) return;
		const next = this.pending;
		this.dirty = false;
		this.discard?.();
		this.pending = null;
		next?.();
	}
}
export const drafts = new DraftGuard();

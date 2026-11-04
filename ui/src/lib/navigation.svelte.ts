/** A single exit decision for every route out of an edited object. */
export class DraftGuard {
	dirty = $state(false);
	saving = $state(false);
	pending = $state<(() => unknown) | null>(null);
	/** The last save from the dialog did not work. */
	failed = $state(false);
	save: (() => Promise<boolean>) | null = null;
	discard: (() => void) | null = null;
	block(action: () => unknown): boolean {
		if (!this.dirty && !this.saving) return false;
		this.pending = action;
		this.failed = false;
		return true;
	}
	async proceed(choice: 'save' | 'discard') {
		if (this.saving) return;
		this.failed = false;
		if (choice === 'save' && !(await this.save?.())) {
			// The editor behind the dialog says why.
			this.failed = true;
			return;
		}
		const next = this.pending;
		this.dirty = false;
		this.discard?.();
		this.pending = null;
		next?.();
	}
}
export const drafts = new DraftGuard();

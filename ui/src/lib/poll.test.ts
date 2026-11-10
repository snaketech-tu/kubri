import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AFTER_CHANGE_MS, changedSomething, every, type Visibility } from './poll';

class Page extends EventTarget implements Visibility {
	hidden = false;
	set(hidden: boolean) {
		this.hidden = hidden;
		this.dispatchEvent(new Event('visibilitychange'));
	}
}

describe('every', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('asks on its timer while the page shows', () => {
		const load = vi.fn();
		const stop = every(30_000, load, new Page());
		vi.advanceTimersByTime(29_999);
		expect(load).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		vi.advanceTimersByTime(60_000);
		expect(load).toHaveBeenCalledTimes(3);
		stop();
		vi.advanceTimersByTime(60_000);
		expect(load).toHaveBeenCalledTimes(3);
	});

	it('asks nothing while hidden, and once when shown again', () => {
		const load = vi.fn();
		const page = new Page();
		const stop = every(30_000, load, page);
		page.set(true);
		vi.advanceTimersByTime(10 * 60_000);
		expect(load).not.toHaveBeenCalled();
		page.set(false);
		expect(load).toHaveBeenCalledTimes(1);
		// The next turn counts from there.
		vi.advanceTimersByTime(29_999);
		expect(load).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(1);
		expect(load).toHaveBeenCalledTimes(2);
		stop();
	});

	it('does not ask when shown again before its turn', () => {
		const load = vi.fn();
		const page = new Page();
		const stop = every(30_000, load, page);
		page.set(true);
		vi.advanceTimersByTime(5000);
		page.set(false);
		expect(load).not.toHaveBeenCalled();
		vi.advanceTimersByTime(25_000);
		expect(load).toHaveBeenCalledTimes(1);
		stop();
	});

	it('asks again soon after a change made in Kubri', () => {
		const load = vi.fn();
		const stop = every(30_000, load, new Page());
		vi.advanceTimersByTime(10_000);
		changedSomething();
		vi.advanceTimersByTime(AFTER_CHANGE_MS);
		expect(load).toHaveBeenCalledTimes(1);
		// And then on its timer again.
		vi.advanceTimersByTime(30_000);
		expect(load).toHaveBeenCalledTimes(2);
		stop();
		changedSomething();
		vi.advanceTimersByTime(AFTER_CHANGE_MS);
		expect(load).toHaveBeenCalledTimes(2);
	});
});

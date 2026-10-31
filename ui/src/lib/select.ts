/** One choice of a select box. Options with the same group follow each other under its heading. */
export interface SelectOption<T> {
	value: T;
	label: string;
	disabled?: boolean;
	group?: string;
}

/** Turns plain values into options that show themselves. */
export function plainOptions<T extends string | number>(values: readonly T[]): SelectOption<T>[] {
	return values.map((value) => ({ value, label: String(value) }));
}

/**
 * The enabled option `delta` places from `from` among `shown` (indexes into the options), skipping
 * disabled ones. Stops at the ends instead of wrapping, like a native select; -1 when none is enabled.
 */
export function step<T>(options: SelectOption<T>[], shown: number[], from: number, delta: number): number {
	const at = shown.indexOf(from);
	if (at < 0) return edge(options, shown, delta < 0);
	const target = at + delta;
	// Positions of the enabled options past the current one, nearest first.
	const ahead = shown
		.map((i, pos) => ({ i, pos }))
		.filter(({ i, pos }) => !options[i].disabled && (delta > 0 ? pos > at : pos < at));
	if (delta < 0) ahead.reverse();
	if (!ahead.length) return from;
	// The furthest one that does not overshoot, or else the nearest one.
	const within = ahead.filter(({ pos }) => (delta > 0 ? pos <= target : pos >= target));
	return (within.length ? within[within.length - 1] : ahead[0]).i;
}

/** The first or last enabled option among `shown`, or -1. */
export function edge<T>(options: SelectOption<T>[], shown: number[], last: boolean): number {
	const enabled = shown.filter((i) => !options[i].disabled);
	return enabled.length ? enabled[last ? enabled.length - 1 : 0] : -1;
}

/**
 * The option that typed letters point at, as native selects find it: the next one after `from` whose
 * label starts with the letters, so pressing the same letter again cycles through them. -1 for none.
 */
export function typeahead<T>(options: SelectOption<T>[], query: string, from: number): number {
	const q = query.toLowerCase();
	if (!q) return -1;
	// One letter pressed again and again cycles; a longer word looks from the current option on.
	const repeated = [...q].every((c) => c === q[0]);
	const needle = repeated ? q[0] : q;
	const start = repeated || from < 0 ? from + 1 : from;
	for (let k = 0; k < options.length; k++) {
		const i = (start + k + options.length) % options.length;
		if (!options[i].disabled && options[i].label.toLowerCase().startsWith(needle)) return i;
	}
	return -1;
}

/** Indexes of the options whose label holds every word of `text`, in their order. */
export function filterOptions<T>(options: SelectOption<T>[], text: string): number[] {
	const words = text.toLowerCase().split(/\s+/).filter(Boolean);
	const all = options.map((_, i) => i);
	if (!words.length) return all;
	return all.filter((i) => {
		const label = options[i].label.toLowerCase();
		return words.every((w) => label.includes(w));
	});
}

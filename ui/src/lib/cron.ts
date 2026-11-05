/**
 * A cron schedule in words, such as "every 5 minutes" or "daily at 02:30", for the common
 * shapes; anything else comes back as it is. Times are the cluster's (UTC unless the CronJob
 * sets a time zone).
 */
export function describeCron(schedule: string): string {
	const text = schedule.trim();
	const macros: Record<string, string> = {
		'@yearly': 'yearly',
		'@annually': 'yearly',
		'@monthly': 'monthly',
		'@weekly': 'weekly',
		'@daily': 'daily',
		'@midnight': 'daily',
		'@hourly': 'hourly'
	};
	if (macros[text]) return macros[text];
	const parts = text.split(/\s+/);
	if (parts.length !== 5) return schedule;
	const [minute, hour, day, month, weekday] = parts;
	const number = (v: string) => /^\d+$/.test(v);
	const step = (v: string) => /^\*\/(\d+)$/.exec(v)?.[1];
	const anyDate = day === '*' && month === '*';
	const at = (h: string, m: string) => `${h.padStart(2, '0')}:${m.padStart(2, '0')}`;

	if (anyDate && weekday === '*' && hour === '*') {
		if (minute === '*') return 'every minute';
		const n = step(minute);
		if (n) return n === '1' ? 'every minute' : `every ${n} minutes`;
		if (number(minute)) return `hourly at :${minute.padStart(2, '0')}`;
	}
	if (anyDate && weekday === '*' && number(minute)) {
		const n = step(hour);
		if (n) return n === '1' ? `hourly at :${minute.padStart(2, '0')}` : `every ${n} hours at :${minute.padStart(2, '0')}`;
		if (number(hour)) return `daily at ${at(hour, minute)}`;
	}
	const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
	if (anyDate && number(minute) && number(hour)) {
		if (weekday === '1-5') return `weekdays at ${at(hour, minute)}`;
		if (number(weekday) && days[Number(weekday)]) return `${days[Number(weekday)]}s at ${at(hour, minute)}`;
	}
	if (month === '*' && weekday === '*' && number(day) && number(minute) && number(hour)) {
		return `monthly on day ${day} at ${at(hour, minute)}`;
	}
	return schedule;
}

import { plural, t } from './i18n';

/**
 * A cron schedule in words, such as "every 5 minutes" or "daily at 02:30", for the common
 * shapes; anything else comes back as it is. Times are the cluster's (UTC unless the CronJob
 * sets a time zone).
 */
export function describeCron(schedule: string): string {
	const text = schedule.trim();
	const macros: Record<string, string> = {
		'@yearly': t('yearly'),
		'@annually': t('yearly'),
		'@monthly': t('monthly'),
		'@weekly': t('weekly'),
		'@daily': t('daily'),
		'@midnight': t('daily'),
		'@hourly': t('hourly')
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
		if (minute === '*') return t('every minute');
		const n = step(minute);
		if (n) return plural(Number(n), 'every minute', 'every {n} minutes');
		if (number(minute)) return t('hourly at :{minute}', { minute: minute.padStart(2, '0') });
	}
	if (anyDate && weekday === '*' && number(minute)) {
		const n = step(hour);
		if (n) return plural(Number(n), 'hourly at :{minute}', 'every {n} hours at :{minute}', { minute: minute.padStart(2, '0') });
		if (number(hour)) return t('daily at {time}', { time: at(hour, minute) });
	}
	if (anyDate && number(minute) && number(hour)) {
		const time = at(hour, minute);
		if (weekday === '1-5') return t('weekdays at {time}', { time });
		const days = [
			t('Sundays at {time}', { time }),
			t('Mondays at {time}', { time }),
			t('Tuesdays at {time}', { time }),
			t('Wednesdays at {time}', { time }),
			t('Thursdays at {time}', { time }),
			t('Fridays at {time}', { time }),
			t('Saturdays at {time}', { time }),
			t('Sundays at {time}', { time })
		];
		if (number(weekday) && days[Number(weekday)]) return days[Number(weekday)];
	}
	if (month === '*' && weekday === '*' && number(day) && number(minute) && number(hour)) {
		return t('monthly on day {day} at {time}', { day, time: at(hour, minute) });
	}
	return schedule;
}

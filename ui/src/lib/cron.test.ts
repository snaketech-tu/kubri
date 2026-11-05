import { describe, expect, it } from 'vitest';
import { describeCron } from './cron';

describe('describeCron', () => {
	it('words the common schedules', () => {
		expect(describeCron('*/1 * * * *')).toBe('every minute');
		expect(describeCron('* * * * *')).toBe('every minute');
		expect(describeCron('*/5 * * * *')).toBe('every 5 minutes');
		expect(describeCron('7 * * * *')).toBe('hourly at :07');
		expect(describeCron('0 */6 * * *')).toBe('every 6 hours at :00');
		expect(describeCron('30 2 * * *')).toBe('daily at 02:30');
		expect(describeCron('0 9 * * 1-5')).toBe('weekdays at 09:00');
		expect(describeCron('0 9 * * 1')).toBe('Mondays at 09:00');
		expect(describeCron('15 3 1 * *')).toBe('monthly on day 1 at 03:15');
		expect(describeCron('@daily')).toBe('daily');
	});
	it('keeps what it cannot word', () => {
		expect(describeCron('0 9 1-7 * 1')).toBe('0 9 1-7 * 1');
		expect(describeCron('5,35 * * * *')).toBe('5,35 * * * *');
		expect(describeCron('CRON_TZ=UTC 0 1 * * *')).toBe('CRON_TZ=UTC 0 1 * * *');
	});
});

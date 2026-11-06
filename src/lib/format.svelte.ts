// How times, dates, durations and sizes are written. Set from
// Settings → Appearance by the layout; the formatters in api.ts read it, so
// everything that shows one follows the choice at once.

export type TimeFormat = 'system' | '12h' | '24h';
export type DateFormat = 'friendly' | 'iso' | 'dmy' | 'mdy' | 'relative';
export type DurationFormat = 'clock' | 'words' | 'seconds';
export type SizeFormat = 'si' | 'iec';

export const fmt = $state({
	time: 'system' as TimeFormat,
	date: 'friendly' as DateFormat,
	duration: 'clock' as DurationFormat,
	size: 'si' as SizeFormat
});

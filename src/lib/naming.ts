// Mirrors src-tauri/src/naming.rs so the settings page can preview a template
// live. Keep the two in step.

export const NAME_TOKENS = ['game', 'date', 'time', 'duration', 'counter', 'hotkey', 'monitor'] as const;
export const DEFAULT_CLIP_TEMPLATE = '{game} {date} {time}';

export interface Sample {
	game: string;
	now: Date;
	durationSecs: number;
	hotkey: string;
	monitor: number;
	counter: number;
}

export function sample(game = 'Valorant'): Sample {
	return { game, now: new Date(), durationSecs: 30, hotkey: 'Alt+F8', monitor: 1, counter: 12 };
}

const p2 = (n: number) => String(n).padStart(2, '0');

function fmtDur(secs: number) {
	const m = Math.floor(secs / 60);
	const s = secs % 60;
	return m === 0 ? `${s}s` : s === 0 ? `${m}m` : `${m}m${s}s`;
}

/** Same replacement rule as the backend: known tokens only, others stay as typed. */
export function expand(template: string, value: (name: string) => string | null): string {
	return template.replace(/\{([^{}]*)\}/g, (whole, raw: string) => value(raw.trim().toLowerCase()) ?? whole);
}

export function sanitize(name: string): string {
	return name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').trim().replace(/\.+$/, '');
}

export function renderName(template: string, c: Sample): string {
	const d = c.now;
	const out = expand(template, (n) => {
		switch (n) {
			case 'game':
				return c.game;
			case 'date':
				return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
			case 'time':
				return `${p2(d.getHours())}-${p2(d.getMinutes())}-${p2(d.getSeconds())}`;
			case 'duration':
				return fmtDur(c.durationSecs);
			case 'counter':
				return String(c.counter).padStart(3, '0');
			case 'hotkey':
				return c.hotkey;
			case 'monitor':
				return `Monitor ${c.monitor}`;
			default:
				return null;
		}
	});
	return sanitize(out) || renderName(DEFAULT_CLIP_TEMPLATE, c);
}

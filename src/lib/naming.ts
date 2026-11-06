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

export type FolderMode = 'flat' | 'game' | 'date' | 'game_date' | 'custom';
export const FOLDER_TOKENS = ['game', 'year', 'month', 'day', 'week'] as const;
export const MAX_FOLDER_DEPTH = 3;

export function folderTemplate(mode: FolderMode, custom: string): string {
	switch (mode) {
		case 'flat':
			return '';
		case 'game':
			return '{game}';
		case 'date':
			return '{year}-{month}-{day}';
		case 'game_date':
			return '{game}/{year}-{month}-{day}';
		case 'custom':
			return custom;
	}
}

function isoWeek(d: Date): number {
	const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
	const day = t.getUTCDay() || 7;
	t.setUTCDate(t.getUTCDate() + 4 - day);
	const y0 = new Date(Date.UTC(t.getUTCFullYear(), 0, 1));
	return Math.ceil(((t.getTime() - y0.getTime()) / 86400000 + 1) / 7);
}

/** Relative subfolder parts, same rules as the backend: cleaned, no `.`/`..`, three deep. */
export function renderFolder(template: string, c: Sample): string[] {
	const d = c.now;
	const parts: string[] = [];
	for (const raw of template.split(/[\\/]/)) {
		const part = sanitize(
			expand(raw, (n) => {
				switch (n) {
					case 'game':
						return c.game;
					case 'year':
						return String(d.getFullYear()).padStart(4, '0');
					case 'month':
						return p2(d.getMonth() + 1);
					case 'day':
						return p2(d.getDate());
					case 'week':
						return p2(isoWeek(d));
					default:
						return null;
				}
			})
		);
		if (part && part !== '.' && part !== '..' && parts.length < MAX_FOLDER_DEPTH) parts.push(part);
	}
	return parts;
}

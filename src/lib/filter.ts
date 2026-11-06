// Library filtering: pure functions so the page stays declarative.
import type { Clip, LibrarySort } from '#lib/api.ts';
import { app } from '#lib/stores/app.svelte.ts';

export type DateRange = 'today' | 'week' | 'month' | 'all' | 'custom';
export type LengthRange = 'any' | 'short' | 'medium' | 'long' | 'xlong';

export interface Filters {
	query: string;
	date: DateRange;
	/** yyyy-mm-dd, inclusive, for 'custom'. */
	from: string;
	to: string;
	length: LengthRange;
	favorites: boolean;
	/** Only clips with at least one smart-highlight moment. */
	highlights: boolean;
	/** A clip must have every one of these tags. */
	tags: string[];
}

export const emptyFilters = (): Filters => ({
	query: '',
	date: 'all',
	from: '',
	to: '',
	length: 'any',
	favorites: false,
	highlights: false,
	tags: []
});

export const isFiltering = (f: Filters) =>
	!!f.query.trim() || f.date !== 'all' || f.length !== 'any' || f.favorites || f.highlights || f.tags.length > 0;

const DAY = 86_400_000;

function dateWindow(f: Filters, now: Date): [number, number] {
	const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
	switch (f.date) {
		case 'today':
			return [midnight, Infinity];
		case 'week':
			return [midnight - 6 * DAY, Infinity];
		case 'month':
			return [midnight - 29 * DAY, Infinity];
		case 'custom': {
			const from = f.from ? new Date(`${f.from}T00:00`).getTime() : -Infinity;
			const to = f.to ? new Date(`${f.to}T00:00`).getTime() + DAY : Infinity;
			return [from, to];
		}
		default:
			return [-Infinity, Infinity];
	}
}

const lengthOk: Record<LengthRange, (s: number) => boolean> = {
	any: () => true,
	short: (s) => s < 10,
	medium: (s) => s >= 10 && s < 30,
	long: (s) => s >= 30 && s < 60,
	xlong: (s) => s >= 60
};

export function applyFilters(clips: Clip[], f: Filters, now = new Date()): Clip[] {
	const words = f.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
	const [from, to] = dateWindow(f, now);
	return clips.filter((c) => {
		if (c.modified < from || c.modified >= to) return false;
		if (!lengthOk[f.length](c.duration)) return false;
		if (f.favorites && !c.favorite) return false;
		if (f.highlights && !c.highlights?.length) return false;
		if (!f.tags.every((t) => c.tags.includes(t))) return false;
		const hay = `${c.name} ${c.tags.join(' ')}`.toLowerCase();
		return words.every((w) => hay.includes(w));
	});
}

/** Every tag used in the library, most used first. */
export function allTags(clips: Clip[]): string[] {
	const n = new Map<string, number>();
	for (const c of clips) for (const t of c.tags) n.set(t, (n.get(t) ?? 0) + 1);
	return [...n].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([t]) => t);
}

/** Library sub-tabs. */
export type LibraryTab = 'all' | 'highlights' | 'games' | 'screenshots' | 'montages' | 'deleted';

/** Smart-highlight moments found, or starred by hand. */
export const isHighlight = (c: Clip) => c.favorite || !!c.highlights?.length;

/** Montage and merge outputs: made by Rewind's own names, or tagged by hand. */
export const isMontage = (c: Clip) =>
	/^Montage\b/i.test(c.name) || / merged$/i.test(c.name) || c.tags.some((t) => /^(montage|merge|merged)$/i.test(t));

/**
 * The game a clip came from. With per-game folders it is the folder; otherwise
 * the name minus its trailing timestamp ("Valorant 2026-10-28 20-11-03"),
 * which is what the default name template produces.
 */
export function gameOf(c: Clip): string {
	const root = app.settings?.clips_dir;
	if (root && c.path.toLowerCase().startsWith(root.toLowerCase())) {
		const parts = c.path.slice(root.length).split(/[\\/]/).filter(Boolean);
		if (parts.length > 1 && /^(game|game_date)$/.test(app.settings?.naming.folder_mode ?? '')) return parts[0];
	}
	const g = c.name.replace(/\s+\d{4}-\d{2}-\d{2}[ _]\d{2}-\d{2}-\d{2}.*$/, '').trim();
	return g && g !== c.name ? g : 'Other';
}

/** `list` newest first (as the library delivers it) reordered by `sort`. */
export function sortClips(list: Clip[], sort: LibrarySort): Clip[] {
	const byNew = (a: Clip, b: Clip) => b.modified - a.modified;
	const cmp: Record<LibrarySort, (a: Clip, b: Clip) => number> = {
		newest: byNew,
		oldest: (a, b) => -byNew(a, b),
		name: (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }) || byNew(a, b),
		duration: (a, b) => b.duration - a.duration || byNew(a, b),
		game: (a, b) => gameOf(a).localeCompare(gameOf(b)) || byNew(a, b),
		size: (a, b) => b.size - a.size || byNew(a, b)
	};
	return sort === 'newest' ? list : [...list].sort(cmp[sort]);
}

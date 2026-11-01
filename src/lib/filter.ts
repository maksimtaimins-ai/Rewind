// Library filtering: pure functions so the page stays declarative.
import type { Clip } from '#lib/api.ts';

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
	tags: []
});

export const isFiltering = (f: Filters) =>
	!!f.query.trim() || f.date !== 'all' || f.length !== 'any' || f.favorites || f.tags.length > 0;

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

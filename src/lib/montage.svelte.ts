// Montage builder helpers. `window` and the timeline mirror
// src-tauri/src/export/montage.rs so the filmstrip shows what gets made.
import type { Clip } from '#lib/api.ts';

export const CROSSFADE = 0.5;
export const MAX_TOTAL = 600;

/** Clips picked in the library before opening the builder. */
export const draft = $state<{ paths: string[] }>({ paths: [] });

/** Part of a clip used: around its best highlight, else its end. */
export function window(c: Clip, max: number | null): { start: number; len: number } {
	const len = Math.max(0, max == null ? c.duration : Math.min(max, c.duration));
	const best = c.highlights?.length ? c.highlights.reduce((a, b) => (b.score > a.score ? b : a)).at : null;
	const start = best != null ? Math.min(Math.max(best - len * 0.6, 0), c.duration - len) : c.duration - len;
	return { start: Math.max(0, start), len };
}

export interface Piece {
	clip: Clip;
	start: number;
	len: number;
	outStart: number;
	fadeIn: number;
}

export function timeline(clips: Clip[], max: number | null, crossfade: boolean): Piece[] {
	const out: Piece[] = [];
	let t = 0;
	for (const clip of clips) {
		const { start, len } = window(clip, max);
		if (len <= 0.05 || t >= MAX_TOTAL) continue;
		const prev = out.at(-1);
		const fadeIn = crossfade && prev ? Math.min(CROSSFADE, prev.len / 2, len / 2) : 0;
		const outStart = Math.max(0, t - fadeIn);
		const l = Math.min(len, MAX_TOTAL - outStart);
		out.push({ clip, start, len: l, outStart, fadeIn });
		t = outStart + l;
	}
	return out;
}

export const totalLength = (p: Piece[]) => (p.length ? p.at(-1)!.outStart + p.at(-1)!.len : 0);

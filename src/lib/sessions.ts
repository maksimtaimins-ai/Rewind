// Group clips into play sessions: clips saved close together belong together.
import type { Clip } from '#lib/api.ts';

export interface Session {
	/** The oldest clip's save time (Unix ms), as a string: stable while clips are added. */
	key: string;
	start: number;
	end: number;
	clips: Clip[];
}

/** `clips` newest first; returns sessions newest first, clips inside newest first. */
export function groupSessions(clips: Clip[], gapMins: number): Session[] {
	const gap = gapMins * 60_000;
	const sorted = [...clips].sort((a, b) => b.modified - a.modified);
	const out: Clip[][] = [];
	for (const c of sorted) {
		const cur = out.at(-1);
		if (cur && cur.at(-1)!.modified - c.modified <= gap) cur.push(c);
		else out.push([c]);
	}
	return out.map((g) => ({ key: String(g.at(-1)!.modified), start: g.at(-1)!.modified, end: g[0].modified, clips: g }));
}

export function defaultSessionName(start: number): string {
	const d = new Date(start);
	const day = String(d.getDate()).padStart(2, '0');
	const month = d.toLocaleDateString('en-GB', { month: 'short' });
	const time = d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
	return `Session — ${day} ${month} ${time}`;
}

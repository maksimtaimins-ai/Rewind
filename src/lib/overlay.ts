// Keystroke overlay layout for the Settings preview. A port of
// src-tauri/src/input/overlay.rs so the preview matches what exports draw.
import type { Keystrokes, PressAnimation } from '#lib/api.ts';

export interface PEvent {
	t: number;
	kind: 'down' | 'up' | 'mouse_down' | 'mouse_up';
	key: string;
	mods?: string[];
	/** Fractions of the frame, for clicks. */
	x?: number;
	y?: number;
}

export interface PCap {
	label: string;
	x: number;
	y: number;
	w: number;
	h: number;
	opacity: number;
	scale: number;
	flash: number;
	held: boolean;
}

export interface PScene {
	unit: number;
	caps: PCap[];
	pluses: { x: number; y: number; opacity: number }[];
	clicks: { x: number; y: number; age: number; opacity: number }[];
}

export const MODIFIERS = ['Ctrl', 'Shift', 'Alt', 'Win'];
const HISTORY = 6;
const CLICK_SECS = 0.5;

export function capWidth(label: string, u: number) {
	if (label === 'Space') return u * 2.6;
	return Math.max(1, [...label].length * 0.25 + 0.55) * u;
}

export function animate(a: PressAnimation, d: number, held: boolean): [number, number] {
	d = Math.max(0, d);
	switch (a) {
		case 'pop':
			return [1 + 0.14 * Math.max(0, 1 - d / 0.15), 0];
		case 'flash':
			return [1, Math.max(0, 1 - d / 0.3)];
		case 'spring':
			return [1 - 0.16 * Math.exp(-9 * d) * Math.cos(28 * d), 0];
		case 'pulse':
			return [held ? 1 + 0.05 * Math.sin(d * Math.PI * 2 * 2.5) : 1, 0];
		default:
			return [1, 0];
	}
}

const ROWS: [number, [string, number][]][] = [
	[0, [['Esc', 1], ['', 1], ['F1', 1], ['F2', 1], ['F3', 1], ['F4', 1], ['', 0.5], ['F5', 1], ['F6', 1], ['F7', 1], ['F8', 1], ['', 0.5], ['F9', 1], ['F10', 1], ['F11', 1], ['F12', 1]]],
	[1.25, [['`', 1], ['1', 1], ['2', 1], ['3', 1], ['4', 1], ['5', 1], ['6', 1], ['7', 1], ['8', 1], ['9', 1], ['0', 1], ['-', 1], ['=', 1], ['Backspace', 2]]],
	[2.25, [['Tab', 1.5], ['Q', 1], ['W', 1], ['E', 1], ['R', 1], ['T', 1], ['Y', 1], ['U', 1], ['I', 1], ['O', 1], ['P', 1], ['[', 1], [']', 1], ['\\', 1.5]]],
	[3.25, [['Caps', 1.75], ['A', 1], ['S', 1], ['D', 1], ['F', 1], ['G', 1], ['H', 1], ['J', 1], ['K', 1], ['L', 1], [';', 1], ["'", 1], ['Enter', 2.25]]],
	[4.25, [['Shift', 2.25], ['Z', 1], ['X', 1], ['C', 1], ['V', 1], ['B', 1], ['N', 1], ['M', 1], [',', 1], ['.', 1], ['/', 1]]],
	[5.25, [['Ctrl', 1.5], ['Win', 1.25], ['Alt', 1.25], ['Space', 6.25]]]
];
const SIDE: Record<string, [number, number]> = {
	insert: [15.25, 1.25], home: [16.25, 1.25], pgup: [17.25, 1.25],
	delete: [15.25, 2.25], end: [16.25, 2.25], pgdn: [17.25, 2.25],
	up: [16.25, 4.25], left: [15.25, 5.25], down: [16.25, 5.25], right: [17.25, 5.25],
	lmb: [18.75, 4.25], mmb: [19.75, 4.25], rmb: [20.75, 4.25]
};

export function boardSpot(key: string): [number, number, number] | null {
	const k = key.toLowerCase();
	for (const [y, keys] of ROWS) {
		let x = 0;
		for (const [name, w] of keys) {
			if (name && name.toLowerCase() === k) return [x, y, w];
			x += w;
		}
	}
	const s = SIDE[k];
	return s ? [s[0], s[1], 1] : null;
}

interface Press {
	key: string;
	mods: string[];
	down: number;
	up: number;
	mouse: boolean;
	pos?: [number, number];
	inCombo: boolean;
}

function presses(events: PEvent[]): Press[] {
	const out: Press[] = [];
	const open: [string, number][] = [];
	for (const e of [...events].sort((a, b) => a.t - b.t)) {
		if (e.kind === 'down' || e.kind === 'mouse_down') {
			open.push([e.key, out.length]);
			out.push({ key: e.key, mods: e.mods ?? [], down: e.t, up: Infinity, mouse: e.kind === 'mouse_down', pos: e.x != null && e.y != null ? [e.x, e.y] : undefined, inCombo: false });
		} else {
			const i = open.findIndex(([k]) => k === e.key);
			if (i >= 0) out[open.splice(i, 1)[0][1]].up = e.t;
		}
	}
	for (const p of out) {
		if (MODIFIERS.includes(p.key)) p.inCombo = out.some((q) => !q.mouse && !MODIFIERS.includes(q.key) && q.down >= p.down && q.down <= p.up);
	}
	return out;
}

/** The overlay at `t` seconds for a `w`x`h` frame. */
export function sceneAt(events: PEvent[], t: number, cfg: Keystrokes, w: number, h: number): PScene {
	const st = cfg.style;
	const k = h / 1080;
	const u = st.size * k;
	const fade = st.fade_secs;
	const pressed = st.pressed_opacity / 100;
	const scene: PScene = { unit: u, caps: [], pluses: [], clicks: [] };
	const list = presses(events);
	const mouseCaps = cfg.position.click_position === 'with_keys';

	if (cfg.filter === 'allow' && cfg.keys.length) {
		const idle = st.idle_opacity / 100;
		const pitch = u * 1.12;
		const keys = [...cfg.keys, ...(mouseCaps && cfg.clicks ? ['LMB', 'MMB', 'RMB'] : [])];
		const spots: [string, number, number, number][] = [];
		const unplaced: string[] = [];
		for (const key of keys) {
			const s = boardSpot(key);
			if (s) spots.push([key, ...s]);
			else unplaced.push(key);
		}
		const bottom = Math.max(...spots.map((s) => s[2]), -Infinity);
		const left = Math.min(...spots.map((s) => s[1]), Infinity);
		let extra = 0;
		for (const key of unplaced) {
			const kw = Math.max(1, capWidth(key, u) / pitch);
			spots.push([key, (left === Infinity ? 0 : left) + extra, bottom === -Infinity ? 0 : bottom + 1, kw]);
			extra += kw;
		}
		const minX = Math.min(...spots.map((s) => s[1]));
		const minY = Math.min(...spots.map((s) => s[2]));
		for (const [key, x, y, kw] of spots) {
			const last = list.filter((p) => p.key === key && p.down <= t).at(-1);
			let opacity = idle, scale = 1, flash = 0, held = false;
			if (last && t < last.up) {
				[scale, flash] = animate(st.animation, t - last.down, true);
				opacity = pressed;
				held = true;
			} else if (last && t - last.up < fade) {
				[scale, flash] = animate(st.animation, t - last.down, false);
				opacity = pressed + (idle - pressed) * ((t - last.up) / fade);
			}
			scene.caps.push({ label: key, x: (x - minX) * pitch, y: (y - minY) * pitch, w: kw * pitch - (pitch - u), h: u, opacity, scale, flash, held });
		}
	} else {
		const visible = list.filter((p) => !p.inCombo && (!p.mouse || mouseCaps) && p.down <= t && t < p.up + fade).slice(-HISTORY);
		const small = u * 0.62;
		const gap = u * 0.12;
		const groups = visible.map((p) => ({ p, mods: MODIFIERS.filter((m) => p.mods.includes(m) && m !== p.key) }));
		const tallest = Math.max(0, ...groups.map((g) => g.mods.length));
		const baseY = st.combo === 'stack' ? tallest * (small + gap) : 0;
		let x = 0;
		groups.forEach(({ p, mods }, gi) => {
			const held = t < p.up;
			const opacity = held ? pressed : pressed * Math.max(0, 1 - (t - p.up) / fade);
			const [scale, flash] = animate(st.animation, t - p.down, held);
			const cap = (label: string, cx: number, cy: number, cw: number, ch: number): PCap => ({ label, x: cx, y: cy, w: cw, h: ch, opacity, scale, flash, held });
			if (st.combo === 'chain') {
				const labels = [...mods, p.key];
				labels.forEach((l, i) => {
					const cw = capWidth(l, u);
					scene.caps.push(cap(l, x, 0, cw, u));
					x += cw;
					if (i + 1 < labels.length) {
						scene.pluses.push({ x: x + u * 0.25, y: u * 0.5, opacity });
						x += u * 0.5;
					}
				});
			} else {
				const gw = Math.max(capWidth(p.key, u), ...mods.map((m) => capWidth(m, small)));
				scene.caps.push(cap(p.key, x, baseY, gw, u));
				mods.forEach((m, i) => scene.caps.push(cap(m, x, baseY - (i + 1) * (small + gap), gw, small)));
				x += gw;
			}
			if (gi + 1 < groups.length) x += u * 0.4;
		});
	}
	place(scene, cfg, w, h, k);

	if (!mouseCaps) {
		for (const p of list.filter((p) => p.mouse && p.down <= t && p.pos)) {
			const end = Math.max(p.up, p.down) + CLICK_SECS;
			if (t >= end) continue;
			scene.clicks.push({ x: p.pos![0] * w, y: p.pos![1] * h, age: t - p.down, opacity: t < p.up ? 1 : 1 - (t - p.up) / CLICK_SECS });
		}
	}
	return scene;
}

/** Box of the laid-out caps, before placing: for dragging. */
export function bounds(s: PScene) {
	if (!s.caps.length) return { x: 0, y: 0, w: 0, h: 0 };
	const x = Math.min(...s.caps.map((c) => c.x));
	const y = Math.min(...s.caps.map((c) => c.y));
	return { x, y, w: Math.max(...s.caps.map((c) => c.x + c.w)) - x, h: Math.max(...s.caps.map((c) => c.y + c.h)) - y };
}

function place(scene: PScene, cfg: Keystrokes, fw: number, fh: number, k: number) {
	if (!scene.caps.length) return;
	const b = bounds(scene);
	const m = cfg.position.margin * k;
	const at: Record<string, [number, number]> = {
		top_left: [m, m],
		top_right: [fw - b.w - m, m],
		bottom_left: [m, fh - b.h - m],
		bottom_right: [fw - b.w - m, fh - b.h - m],
		top_center: [(fw - b.w) / 2, m],
		bottom_center: [(fw - b.w) / 2, fh - b.h - m],
		custom: [cfg.position.x * fw, cfg.position.y * fh]
	};
	const [px, py] = at[cfg.position.anchor];
	const dx = Math.min(Math.max(px, 0), Math.max(fw - b.w, 0)) - b.x;
	const dy = Math.min(Math.max(py, 0), Math.max(fh - b.h, 0)) - b.y;
	for (const c of scene.caps) {
		c.x += dx;
		c.y += dy;
	}
	for (const p of scene.pluses) {
		p.x += dx;
		p.y += dy;
	}
}

/** Name of a browser key event the way Rewind records it (for the key picker). */
export function keyName(e: KeyboardEvent): string | null {
	const c = e.code;
	if (/^Key[A-Z]$/.test(c)) return c.slice(3);
	if (/^Digit\d$/.test(c)) return c.slice(5);
	if (/^F\d{1,2}$/.test(c)) return c;
	if (/^Numpad\d$/.test(c)) return `Num${c.slice(6)}`;
	const map: Record<string, string> = {
		Space: 'Space', Enter: 'Enter', Tab: 'Tab', Escape: 'Esc', Backspace: 'Backspace', CapsLock: 'Caps',
		ShiftLeft: 'Shift', ShiftRight: 'Shift', ControlLeft: 'Ctrl', ControlRight: 'Ctrl', AltLeft: 'Alt', AltRight: 'Alt',
		MetaLeft: 'Win', MetaRight: 'Win', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
		PageUp: 'PgUp', PageDown: 'PgDn', Home: 'Home', End: 'End', Insert: 'Insert', Delete: 'Delete',
		Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';',
		Quote: "'", Comma: ',', Period: '.', Slash: '/', NumpadMultiply: 'Num*', NumpadAdd: 'Num+', NumpadSubtract: 'Num-',
		NumpadDecimal: 'Num.', NumpadDivide: 'Num/', NumpadEnter: 'Enter', ContextMenu: 'Menu', PrintScreen: 'PrtSc', Pause: 'Pause'
	};
	return map[c] ?? null;
}

/** A few seconds of made-up input that loops in the preview. */
export function demo(cfg: Keystrokes): { events: PEvent[]; length: number } {
	if (cfg.filter === 'allow' && cfg.keys.length) {
		const keys = cfg.keys.slice(0, 4);
		const events: PEvent[] = keys.flatMap((k, i) => [
			{ t: 0.3 + i * 0.55, kind: 'down' as const, key: k },
			{ t: 0.3 + i * 0.55 + 0.35, kind: 'up' as const, key: k }
		]);
		if (cfg.clicks) events.push({ t: 2.6, kind: 'mouse_down', key: 'LMB', x: 0.62, y: 0.42 }, { t: 2.75, kind: 'mouse_up', key: 'LMB' });
		return { events, length: Math.max(3.4, 0.3 + keys.length * 0.55 + Math.min(cfg.style.fade_secs, 2) + 0.4) };
	}
	const events: PEvent[] = [
		{ t: 0.2, kind: 'down', key: 'Ctrl' },
		{ t: 0.3, kind: 'down', key: 'Shift', mods: ['Ctrl'] },
		{ t: 0.4, kind: 'down', key: 'P', mods: ['Ctrl', 'Shift'] },
		{ t: 0.9, kind: 'up', key: 'P' },
		{ t: 0.95, kind: 'up', key: 'Shift' },
		{ t: 1.0, kind: 'up', key: 'Ctrl' },
		{ t: 1.4, kind: 'down', key: 'W' },
		{ t: 2.6, kind: 'up', key: 'W' },
		{ t: 1.9, kind: 'down', key: 'Space' },
		{ t: 2.05, kind: 'up', key: 'Space' }
	];
	if (cfg.clicks) events.push({ t: 2.2, kind: 'mouse_down', key: 'LMB', x: 0.62, y: 0.42 }, { t: 2.35, kind: 'mouse_up', key: 'LMB' });
	return { events, length: 2.8 + Math.min(cfg.style.fade_secs, 2.5) };
}

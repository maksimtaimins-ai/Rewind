// Global search: one flat list of things you can jump to, ranked by a simple
// word match. Pure functions; the palette component owns the UI.
import type { Clip, Settings } from '#lib/api.ts';
import { gameOf } from '#lib/filter.ts';

export type Group = 'Clips' | 'Settings' | 'Help';

export interface Item {
	id: string;
	group: Group;
	title: string;
	/** One line under the title. */
	sub?: string;
	/** Extra words that should match but aren't shown. */
	keywords?: string;
	/** Where it goes: a route, with a #hash for a setting. */
	to: string;
	/** A clip's thumbnail path. */
	thumb?: string | null;
}

/** The anchor id `Row` gives a setting, from its label. */
export const slug = (label: string) => 's-' + label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');

interface Setting {
	label: string;
	hint: string;
	words?: string;
	/** Row id when the label's slug isn't unique. */
	id?: string;
	/** Lives under Advanced, which only exists in Pro mode. */
	pro?: boolean;
}

const settings: Setting[] = [
	{ label: 'Buffer length', hint: 'How many seconds of your screen Rewind keeps in memory', words: 'replay length duration memory ram' },
	{ label: 'Save hotkey', hint: 'The key that saves the last seconds as a clip', words: 'shortcut keybind' },
	{ label: 'Save folder', hint: 'Where clips are written', words: 'directory path location clips dir' },
	{ label: 'Encoder', hint: 'Automatic or a specific graphics card encoder', words: 'nvenc amf quick sync hardware software gpu', pro: true },
	{ label: 'Pro mode', hint: 'Show encoder, bitrate, codec and other advanced settings', words: 'advanced expert' },
	{ label: 'Mark this moment', hint: 'Hotkey that drops a marker to trim around later', words: 'marker hotkey' },
	{ label: 'Screenshot', hint: 'Hotkey that saves the current frame as a PNG', words: 'screen capture png hotkey' },
	{ label: 'Favorite last clip', hint: 'Hotkey that stars your most recent clip', words: 'star hotkey' },
	{ label: 'Extend last clip', hint: 'Hotkey that adds footage before your last clip', words: 'longer hotkey' },
	{ label: 'Slow-mo replay', hint: 'Hotkey for a slowed-down replay of the last seconds', words: 'slow motion hotkey' },
	{ label: 'Slow-mo length', hint: 'How much of the buffer to slow down', words: 'slow motion seconds' },
	{ label: 'Slow-mo speed', hint: 'How slow the replay plays', words: 'slow motion' },
	{ label: 'Compact HUD', hint: 'Hotkey that switches between the floating pill and this window', words: 'overlay pill hotkey' },
	{ label: 'Compact HUD', hint: 'Swap this window for a small always-on-top pill', id: 's-compact-hud-toggle', words: 'overlay pill appearance' },
	{ label: 'Theme', hint: 'Follow Windows, or pick a colour theme', id: 'appearance', words: 'dark light colour color appearance' },
	{ label: 'Open minimized to the tray', hint: 'The window stays hidden until you open it', words: 'startup tray hidden' },
	{ label: 'Play a sound on save', hint: 'A short click when a clip is written', words: 'audio beep notification' },
	{ label: 'Record extra after press', hint: 'Keeps recording a few seconds after you save', words: 'post-roll postroll after' },
	{ label: 'Extend last clip by', hint: 'Footage added before your last clip', words: 'seconds extend' },
	{ label: 'Resolution', hint: 'Height of saved clips; lower makes smaller files', words: '720p 1080p 1440p native size' },
	{ label: 'Frame rate', hint: 'Frames per second cap for clips', words: 'fps 30 60 120 144' },
	{ label: 'Bitrate', hint: 'How much data each second of video gets', words: 'quality size mbps', pro: true },
	{ label: 'Record the mouse cursor', hint: 'Draws the pointer into clips', words: 'pointer' },
	{ label: 'Launch when Windows starts', hint: 'Starts in the tray with the buffer running', words: 'startup boot autostart' },
	{ label: 'Screen', hint: 'The monitor Rewind records', words: 'monitor display' },
	{ label: 'Group into sessions', hint: 'Clips saved this close together show as one session', words: 'library gap minutes' },
	{ label: 'Keep deleted clips for', hint: 'Days before Recently deleted is emptied', words: 'trash purge recycle cleanup' },
	{ label: 'Free on the clips drive', hint: 'Disk space left, and the storage breakdown', id: 'storage', words: 'disk space storage' },
	{ label: 'Send clips between PCs', hint: 'LAN send to other PCs running Rewind', words: 'lan network share' },
	{ label: 'Discord webhooks', hint: 'Post a clip straight into a Discord channel', id: 'sharing', words: 'discord share upload webhook' },
	{ label: 'Watch folders', hint: 'Export new clips from a folder automatically', id: 'watch', words: 'export automatic folder' },
	{ label: 'Game profiles', hint: 'Per-game frame rate, bitrate, hotkeys and save folder', id: 'games', words: 'games per-game profile detect' },
	{ label: 'Tour of the Live tab', hint: 'Replay the four-step intro', words: 'help tutorial onboarding' },
	{ label: 'Updates', hint: 'Check GitHub Releases for a newer version', words: 'update upgrade version' },
	{ label: 'Codec', hint: 'H.264 plays everywhere; HEVC is smaller', words: 'h264 h265 hevc', pro: true },
	{ label: 'Rate control', hint: 'How the bitrate is spent: variable, constant or quality', words: 'vbr cbr', pro: true },
	{ label: 'Keyframe every', hint: 'Clips start on keyframes; shorter is more precise', words: 'interval gop trim', pro: true },
	{ label: 'Memory limit', hint: 'Oldest seconds are dropped first above this', words: 'ram buffer cap', pro: true },
	{ label: 'Overlay screen', hint: 'Where the live keystrokes and HUD appear', words: 'monitor display', pro: true },
	{ label: 'Low disk warning', hint: 'Warn on the Live tab below this much free space', words: 'storage', pro: true },
	{ label: 'Test current settings', hint: 'Measure 10 seconds of real capture', id: 's-test-current-settings', words: 'benchmark performance', pro: true },
	{ label: 'Encoder benchmark', hint: 'Compare every encoder on this PC', words: 'benchmark speed', pro: true },
	{ label: 'Smart highlights', hint: 'Find loud moments in clips by listening to audio', words: 'highlights audio analyze experimental', pro: true },
	{ label: 'Share settings', hint: 'Export or import a settings file', words: 'export import backup', pro: true },
	{ label: 'Performance', hint: 'What each optional feature costs right now', id: 'performance', words: 'diagnostics cpu ram usage', pro: true }
];

export function settingItems(pro: boolean): Item[] {
	return settings
		.filter((s) => pro || !s.pro)
		.map((s) => ({
			id: 'setting:' + (s.id ?? slug(s.label)) + s.hint.length,
			group: 'Settings' as const,
			title: s.label,
			sub: s.hint,
			keywords: s.words,
			to: '/settings#' + (s.id ?? slug(s.label))
		}));
}

interface Topic {
	title: string;
	text: string;
	words?: string;
	to: string;
}

const topics: Topic[] = [
	{ title: 'Save a clip', text: 'Press the save hotkey to keep the last seconds of your screen.', words: 'hotkey buffer record', to: '/settings#s-save-hotkey' },
	{ title: 'Mark a moment, trim later', text: 'Drop a marker, then pick how much to keep from the Live tab.', words: 'marker bookmark', to: '/' },
	{ title: 'Slow-mo replay', text: 'A slowed-down replay of the last few seconds, video only.', words: 'slow motion', to: '/settings#s-slow-mo-replay' },
	{ title: 'Trim a clip', text: 'Open any clip to cut it to the part you want.', words: 'cut edit scrubber', to: '/library' },
	{ title: 'Merge clips', text: 'Select 2 to 5 clips in Clips, then Merge.', words: 'join combine', to: '/library' },
	{ title: 'Make a montage', text: 'String clips into one video with cuts or crossfades, plus music.', words: 'compilation video', to: '/library/montage' },
	{ title: 'Tags and favorites', text: 'Star clips and tag them to find them later.', words: 'star label organize', to: '/library' },
	{ title: 'Recently deleted', text: 'Deleted clips wait here before they are removed for good. Restore any time.', words: 'trash restore undelete recycle', to: '/library?tab=deleted' },
	{ title: 'Screenshots', text: 'Saved frames live in Clips under Screenshots.', words: 'png image', to: '/library?tab=screenshots' },
	{ title: 'Share to Discord or another PC', text: 'Right-click a clip to post it to a webhook or send it over your network.', words: 'send upload lan', to: '/settings#sharing' },
	{ title: 'Keystroke overlay', text: 'Record the keys you press and draw them into exports.', words: 'keyboard input keys', to: '/settings#capture' },
	{ title: 'Privacy: pause for apps', text: 'Capture pauses while a listed app, like a password manager, is focused.', words: 'private pause password', to: '/settings#capture' },
	{ title: 'Game profiles', text: 'Rewind switches settings when it recognises a game.', words: 'detect per-game', to: '/settings#games' },
	{ title: 'Smart highlights', text: 'Listens to clip audio for loud spikes and marks them on the trim timeline.', words: 'audio analyze moments', to: '/settings#smart-highlights' },
	{ title: 'Where do clips go?', text: 'Clips save to your Save folder; open it from Clips.', words: 'folder location disk', to: '/settings#s-save-folder' },
	{ title: 'Buffer and memory', text: 'Rewind keeps recent video in RAM, never on disk, until you save.', words: 'ram memory how it works', to: '/settings#s-buffer-length' }
];

export function helpItems(s: Settings | null): Item[] {
	const keys: [string, string | undefined, string][] = [
		['Save clip', s?.hotkeys.save_clip, '/settings#s-save-hotkey'],
		['Mark this moment', s?.hotkeys.marker, '/settings#s-mark-this-moment'],
		['Screenshot', s?.hotkeys.screenshot, '/settings#s-screenshot'],
		['Favorite last clip', s?.hotkeys.favorite_last, '/settings#s-favorite-last-clip'],
		['Extend last clip', s?.hotkeys.extend_last, '/settings#s-extend-last-clip'],
		['Slow-mo replay', s?.hotkeys.slow_mo, '/settings#s-slow-mo-replay'],
		['Compact HUD', s?.hotkeys.compact_hud, '/settings#s-compact-hud']
	];
	return [
		...keys.map(([name, key, to]) => ({
			id: 'help:key:' + name,
			group: 'Help' as const,
			title: `${name} hotkey`,
			sub: key ? `Currently ${key}. Change it in Settings.` : 'Not set. Set one in Settings.',
			keywords: `shortcut keybind key ${key ?? ''}`,
			to
		})),
		...topics.map((t) => ({ id: 'help:' + t.title, group: 'Help' as const, title: t.title, sub: t.text, keywords: t.words, to: t.to }))
	];
}

const MONTHS = ['january', 'february', 'march', 'april', 'may', 'june', 'july', 'august', 'september', 'october', 'november', 'december'];

export function clipItems(clips: Clip[]): Item[] {
	return clips.map((c) => {
		const d = new Date(c.modified);
		const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
		const month = MONTHS[d.getMonth()];
		const game = gameOf(c);
		return {
			id: 'clip:' + c.path,
			group: 'Clips' as const,
			title: c.name,
			sub: [game !== 'Other' ? game : null, d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }), ...c.tags.map((t) => '#' + t)].filter(Boolean).join('  ·  '),
			keywords: `${game} ${c.tags.join(' ')} ${iso} ${month} ${month.slice(0, 3)} ${d.getDate()} ${c.favorite ? 'favorite starred' : ''}`,
			to: `/trim?path=${encodeURIComponent(c.path)}`,
			thumb: c.thumb
		};
	});
}

/** 0 = no match. Every word must appear; earlier and whole-word hits score higher. */
function score(it: Item, words: string[]): number {
	const title = it.title.toLowerCase();
	const rest = `${it.sub ?? ''} ${it.keywords ?? ''}`.toLowerCase();
	let total = 0;
	for (const w of words) {
		let s = 0;
		const i = title.indexOf(w);
		if (i === 0) s = 100;
		else if (i > 0) s = title[i - 1] === ' ' ? 80 : 50;
		else if (rest.includes(w)) s = 20;
		if (!s) return 0;
		total += s;
	}
	return total;
}

const LIMITS: Record<Group, number> = { Clips: 6, Settings: 6, Help: 5 };

/** Ranked hits, grouped in a fixed order. */
export function search(items: Item[], query: string): Item[] {
	const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
	if (!words.length) return [];
	const out: Item[] = [];
	for (const g of ['Clips', 'Settings', 'Help'] as Group[]) {
		out.push(
			...items
				.filter((i) => i.group === g)
				.map((i) => [i, score(i, words)] as const)
				.filter(([, s]) => s > 0)
				.sort((a, b) => b[1] - a[1])
				.slice(0, LIMITS[g])
				.map(([i]) => i)
		);
	}
	return out;
}

/** What shows before you type: the newest clips and the settings people change most. */
export function suggestions(items: Item[]): Item[] {
	const pick = (g: Group, n: number, titles?: string[]) =>
		titles ? titles.map((t) => items.find((i) => i.group === g && i.title === t)).filter((i): i is Item => !!i) : items.filter((i) => i.group === g).slice(0, n);
	return [...pick('Clips', 3), ...pick('Settings', 0, ['Buffer length', 'Save hotkey', 'Save folder', 'Encoder']), ...pick('Help', 2)];
}

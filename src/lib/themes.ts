// Themes: each is one base scale plus an accent, applied as CSS custom
// properties on <html>. app.css holds the Dark values as the pre-JS default.

export interface Palette {
	scheme: 'dark' | 'light';
	ground: string;
	raised: string;
	sunken: string;
	line: string;
	lineStrong: string;
	text: string;
	muted: string;
	faint: string;
	accent: string;
	onAccent: string;
	/** Second accent (markers). Same as `accent` unless the theme has two. */
	accent2?: string;
	/** Recording dot. */
	signal: string;
}

export interface Theme {
	id: string;
	name: string;
	palette: Palette;
}

const dark: Palette = {
	scheme: 'dark',
	ground: '#141517',
	raised: '#1b1c1f',
	sunken: '#0f1012',
	line: '#2a2b2f',
	lineStrong: '#3a3b40',
	text: '#e7e7e5',
	muted: '#8b8c90',
	faint: '#5a5b60',
	accent: '#e7e7e5',
	onAccent: '#141517',
	signal: '#ff4d3d'
};

const light: Palette = {
	scheme: 'light',
	ground: '#f4f5f6',
	raised: '#eaebed',
	sunken: '#e4e5e8',
	line: '#d9dadd',
	lineStrong: '#c4c5c9',
	text: '#17181a',
	muted: '#63656b',
	faint: '#9a9ca1',
	accent: '#17181a',
	onAccent: '#f4f5f6',
	signal: '#e5352a'
};

export const themes: Theme[] = [
	{ id: 'dark', name: 'Dark', palette: dark },
	{ id: 'light', name: 'Light', palette: light },
	{
		id: 'midnight',
		name: 'Midnight',
		palette: {
			scheme: 'dark',
			ground: '#0e1118',
			raised: '#151a24',
			sunken: '#0a0d13',
			line: '#222a38',
			lineStrong: '#313b4d',
			text: '#dfe5ee',
			muted: '#8792a5',
			faint: '#566074',
			accent: '#8fb3d9',
			onAccent: '#0e1118',
			signal: '#ff5a4a'
		}
	},
	{
		id: 'forest',
		name: 'Forest',
		palette: {
			scheme: 'dark',
			ground: '#1a1917',
			raised: '#22211e',
			sunken: '#141311',
			line: '#2f2d29',
			lineStrong: '#403d37',
			text: '#e6e3dc',
			muted: '#8f8a80',
			faint: '#5e5a52',
			accent: '#8fb08a',
			onAccent: '#141311',
			signal: '#ff5a45'
		}
	},
	{
		id: 'sunset',
		name: 'Sunset',
		palette: {
			scheme: 'dark',
			ground: '#1c1614',
			raised: '#251d1a',
			sunken: '#151110',
			line: '#352a25',
			lineStrong: '#47382f',
			text: '#f0e4dc',
			muted: '#a08d82',
			faint: '#6b5b52',
			accent: '#f29a5e',
			onAccent: '#1c1614',
			signal: '#ff3d5e'
		}
	},
	{
		id: 'rose',
		name: 'Rose',
		palette: {
			scheme: 'dark',
			ground: '#1a1517',
			raised: '#231c1f',
			sunken: '#141012',
			line: '#33292d',
			lineStrong: '#45383d',
			text: '#efe3e7',
			muted: '#a08c93',
			faint: '#6b5a60',
			accent: '#d99aab',
			onAccent: '#1a1517',
			signal: '#ff4d3d'
		}
	},
	{
		id: 'ocean',
		name: 'Ocean',
		palette: {
			scheme: 'dark',
			ground: '#0f1617',
			raised: '#152022',
			sunken: '#0b1112',
			line: '#213032',
			lineStrong: '#2f4245',
			text: '#dde9ea',
			muted: '#82999b',
			faint: '#536769',
			accent: '#4fc1b6',
			onAccent: '#0b1112',
			signal: '#ff5a4a'
		}
	},
	{
		id: 'lavender',
		name: 'Lavender',
		palette: {
			scheme: 'dark',
			ground: '#16141b',
			raised: '#1e1b25',
			sunken: '#110f15',
			line: '#2c2836',
			lineStrong: '#3c3749',
			text: '#e8e4f0',
			muted: '#958ea5',
			faint: '#615b70',
			accent: '#b9a3e8',
			onAccent: '#16141b',
			signal: '#ff4d4d'
		}
	},
	{
		id: 'amber',
		name: 'Amber',
		palette: {
			scheme: 'dark',
			ground: '#171512',
			raised: '#201d18',
			sunken: '#12100d',
			line: '#2e2a23',
			lineStrong: '#3f3a30',
			text: '#ede6d8',
			muted: '#9b9282',
			faint: '#665f52',
			accent: '#f2b544',
			onAccent: '#171512',
			signal: '#ff4d3d'
		}
	},
	{
		id: 'mono',
		name: 'Mono',
		palette: {
			scheme: 'dark',
			ground: '#121212',
			raised: '#1c1c1c',
			sunken: '#0c0c0c',
			line: '#2a2a2a',
			lineStrong: '#3b3b3b',
			text: '#e6e6e6',
			muted: '#8c8c8c',
			faint: '#5c5c5c',
			accent: '#ffffff',
			onAccent: '#121212',
			signal: '#ffffff'
		}
	},
	{
		id: 'nord',
		name: 'Nord',
		palette: {
			scheme: 'dark',
			ground: '#2e3440',
			raised: '#3b4252',
			sunken: '#272c36',
			line: '#434c5e',
			lineStrong: '#4c566a',
			text: '#eceff4',
			muted: '#a7b0c0',
			faint: '#7b8598',
			accent: '#88c0d0',
			onAccent: '#2e3440',
			accent2: '#a3be8c',
			signal: '#bf616a'
		}
	},
	{
		id: 'dracula',
		name: 'Dracula',
		palette: {
			scheme: 'dark',
			ground: '#282a36',
			raised: '#343746',
			sunken: '#21222c',
			line: '#44475a',
			lineStrong: '#565a70',
			text: '#f8f8f2',
			muted: '#b0b3c7',
			faint: '#6272a4',
			accent: '#bd93f9',
			onAccent: '#282a36',
			accent2: '#ff79c6',
			signal: '#ff5555'
		}
	},
	{
		id: 'catppuccin-mocha',
		name: 'Catppuccin Mocha',
		palette: {
			scheme: 'dark',
			ground: '#1e1e2e',
			raised: '#313244',
			sunken: '#181825',
			line: '#45475a',
			lineStrong: '#585b70',
			text: '#cdd6f4',
			muted: '#a6adc8',
			faint: '#7f849c',
			accent: '#cba6f7',
			onAccent: '#1e1e2e',
			accent2: '#89b4fa',
			signal: '#f38ba8'
		}
	},
	{
		id: 'cyberpunk',
		name: 'Cyberpunk',
		palette: {
			scheme: 'dark',
			ground: '#0a0a10',
			raised: '#13121c',
			sunken: '#060609',
			line: '#24203a',
			lineStrong: '#3a3060',
			text: '#eae6ff',
			muted: '#9a92c4',
			faint: '#5e5880',
			accent: '#ff2bd6',
			onAccent: '#0a0a10',
			accent2: '#00f0ff',
			signal: '#00f0ff'
		}
	},
	{
		id: 'high-contrast',
		name: 'High contrast',
		palette: {
			scheme: 'dark',
			ground: '#000000',
			raised: '#1a1a1a',
			sunken: '#000000',
			line: '#ffffff',
			lineStrong: '#ffffff',
			text: '#ffffff',
			muted: '#e6e6e6',
			faint: '#c4c4c4',
			accent: '#ffff00',
			onAccent: '#000000',
			signal: '#ff5c5c'
		}
	}
];

/** The "Follow system" choice, listed first in the picker. */
export const SYSTEM = 'system';

export function findTheme(id: string): Theme {
	return themes.find((t) => t.id === id) ?? themes[0];
}

/** Resolve the saved choice to a palette, following Windows for "system". */
export function resolve(id: string, prefersLight: boolean): Palette {
	if (id === SYSTEM) return prefersLight ? light : dark;
	return findTheme(id).palette;
}

export function apply(p: Palette) {
	const root = document.documentElement;
	const vars: Record<string, string> = {
		ground: p.ground,
		raised: p.raised,
		sunken: p.sunken,
		line: p.line,
		'line-strong': p.lineStrong,
		text: p.text,
		muted: p.muted,
		faint: p.faint,
		accent: p.accent,
		'on-accent': p.onAccent,
		'accent-2': p.accent2 ?? p.accent,
		signal: p.signal,
		// Smart-highlight ticks: one amber per scheme, readable on every ground.
		mark: p.scheme === 'dark' ? '#ff9f2e' : '#c46a00'
	};
	for (const [k, v] of Object.entries(vars)) root.style.setProperty(`--${k}`, v);
	root.style.colorScheme = p.scheme;
	root.dataset.scheme = p.scheme;
}

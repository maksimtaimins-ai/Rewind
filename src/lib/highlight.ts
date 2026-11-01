import type { Highlight } from './api.ts';

const presets: Record<Exclude<Highlight['color'], 'off' | 'custom'>, string> = {
	blue: '#0090ff',
	green: '#30a46c',
	red: '#e5484d'
};

/** The CSS color a highlight draws in, or null when it's off. */
export function highlightColor(h: Highlight): string | null {
	if (h.color === 'off') return null;
	return h.color === 'custom' ? h.custom_color : presets[h.color];
}

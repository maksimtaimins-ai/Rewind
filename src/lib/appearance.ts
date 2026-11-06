// Typeface and UI scale, applied from settings. Colours live in themes.ts.
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { inApp } from '#lib/stores/app.svelte.ts';

const stacks: Record<string, string> = {
	inter: "'Inter Variable', system-ui, sans-serif",
	mono: "'Martian Mono', ui-monospace, monospace",
	system: "system-ui, 'Segoe UI', sans-serif"
};

/** `theme` removes the override, leaving the stylesheet's own face. */
export function applyFont(font: string) {
	const root = document.documentElement.style;
	if (stacks[font]) root.setProperty('--font-sans', stacks[font]);
	else root.removeProperty('--font-sans');
}

let lastScale = 100;
/** Whole-window zoom through the webview, so layout and viewport units stay honest. */
export function applyScale(percent: number) {
	if (percent === lastScale) return;
	lastScale = percent;
	if (inApp) getCurrentWebview().setZoom(percent / 100).catch(() => {});
	else document.documentElement.style.zoom = String(percent / 100);
}

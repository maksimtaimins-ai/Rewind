import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { api, type Clip, type LibraryMeta, type Settings, type Status } from '#lib/api.ts';
import { mockClips, mockSettings, mockStatus } from './mock.ts';

export type ToastLevel = 'info' | 'success' | 'warning' | 'error';
export interface Toast {
	id: number;
	level: ToastLevel;
	message: string;
}

export const inApp = typeof window !== 'undefined' && isTauri();

export const app = $state({
	ready: false,
	settings: null as Settings | null,
	status: null as Status | null,
	clips: [] as Clip[],
	meta: { tag_colors: {}, sessions: {} } as LibraryMeta,
	toasts: [] as Toast[],
	lastSaved: null as Clip | null
});

let toastId = 0;
export function toast(level: ToastLevel, message: string) {
	const id = ++toastId;
	app.toasts = [...app.toasts.slice(-3), { id, level, message }];
	setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), level === 'error' ? 8000 : 4000);
}

export async function refreshClips() {
	if (!inApp) {
		app.clips = mockClips();
		return;
	}
	[app.clips, app.meta] = await Promise.all([api.clips(), api.libraryMeta()]);
}

let started = false;
export async function init() {
	if (started) return;
	started = true;
	if (!inApp) {
		// Browser preview (vite dev without Tauri): render with sample data.
		app.settings = mockSettings();
		app.status = mockStatus();
		app.clips = mockClips();
		app.ready = true;
		return;
	}
	const [settings, status] = await Promise.all([api.getSettings(), api.getStatus()]);
	app.settings = settings;
	app.status = status;
	await refreshClips();
	await listen<Status>('status', (e) => (app.status = e.payload));
	await listen<Clip>('clip-saved', (e) => {
		app.lastSaved = e.payload;
		app.clips = [e.payload, ...app.clips.filter((c) => c.path !== e.payload.path)];
	});
	await listen('library-changed', () => refreshClips());
	await listen<{ level: ToastLevel; message: string }>('notice', (e) =>
		toast(e.payload.level, e.payload.message)
	);
	app.ready = true;
}

/** Star or unstar a clip, updating the list right away. */
export async function setFavorite(path: string, favorite: boolean) {
	app.clips = app.clips.map((c) => (c.path === path ? { ...c, favorite } : c));
	if (!inApp) return;
	try {
		await api.setFavorite(path, favorite);
	} catch (e) {
		toast('error', String(e));
		await refreshClips();
	}
}

/** Apply a change to settings and persist it. */
export async function updateSettings(change: (s: Settings) => void) {
	if (!app.settings) return;
	const next = $state.snapshot(app.settings) as Settings;
	change(next);
	app.settings = next;
	if (!inApp) return;
	try {
		await api.setSettings(next);
	} catch (e) {
		toast('error', `Couldn't save settings: ${e}`);
	}
}

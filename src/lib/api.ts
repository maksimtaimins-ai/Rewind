import { invoke, convertFileSrc } from '@tauri-apps/api/core';

export type Codec = 'h264' | 'hevc';
export type EncoderChoice = 'auto' | 'nvenc' | 'amf' | 'qsv' | 'software';
export type RateControl = 'cbr' | 'vbr' | 'quality';
export type HdrMode = 'tonemap_to_sdr' | 'passthrough';

export type AudioSource =
	| { kind: 'system' }
	| { kind: 'system_excluding'; exe: string }
	| { kind: 'app'; exe: string }
	| { kind: 'mic'; device_id: string | null };

export interface AudioTrack {
	name: string;
	enabled: boolean;
	source: AudioSource;
	volume: number;
}

export interface GameProfile {
	exe: string;
	name: string;
	fps: number | null;
	height: number | null;
	bitrate_kbps: number | null;
	buffer_seconds: number | null;
}

export interface Hotkeys {
	save_clip: string;
	marker: string;
	screenshot: string;
	extend_last: string;
	slow_mo: string;
	favorite_last: string;
}

export interface BufferSlot {
	enabled: boolean;
	seconds: number;
	hotkey: string;
}

export interface Settings {
	version: number;
	first_run_complete: boolean;
	pro_mode: boolean;
	/** 'system' or a theme id from themes.ts. */
	theme: string;
	clips_dir: string;
	launch_on_startup: boolean;
	start_minimized: boolean;
	play_sound_on_save: boolean;
	video: {
		monitor: number;
		height: number;
		fps: number;
		codec: Codec;
		encoder: EncoderChoice;
		rate_control: RateControl;
		bitrate_kbps: number;
		quality: number;
		keyframe_secs: number;
		capture_cursor: boolean;
		hdr: HdrMode;
	};
	buffer: {
		seconds: number;
		/** Slots B and C; slot A is `seconds` with `hotkeys.save_clip`. */
		extra_slots: BufferSlot[];
		max_memory_mb: number;
		extend_secs: number;
	};
	slow_mo: { seconds: 2 | 3 | 5; speed: number };
	library: { session_gap_mins: number; purge_days: number };
	audio: AudioTrack[];
	hotkeys: Hotkeys;
	profiles: GameProfile[];
	privacy_apps: string[];
	ssd_warn_gb_per_day: number;
}

export type EngineState = 'idle' | 'recording' | 'paused' | 'error';

export interface Status {
	state: EngineState;
	buffered_secs: number;
	buffer_secs: number;
	/** Every enabled replay length, slot A first. */
	slots: SlotStatus[];
	fps: number;
	bitrate_kbps: number;
	dropped: number;
	memory_mb: number;
	encoder: string | null;
	resolution: [number, number] | null;
	game: string | null;
	privacy: boolean;
	/** Markers set this session, oldest first. */
	markers: MarkerInfo[];
	error: string | null;
	written_today_gb: number;
}

export interface SlotStatus {
	name: string;
	seconds: number;
	held: number;
	hotkey: string;
}

export interface MarkerInfo {
	id: number;
	/** Seconds since it was set. */
	ago: number;
	/** Unix milliseconds. */
	at_ms: number;
	/** The buffer still holds footage at the marker. */
	in_buffer: boolean;
}

export interface Clip {
	path: string;
	name: string;
	size: number;
	modified: number;
	duration: number;
	thumb: string | null;
	/** Changes when the thumbnail file is replaced. */
	thumb_rev: number;
	favorite: boolean;
	tags: string[];
}

export type ExportPreset = 'discord10' | 'discord25' | 'shorts' | 'twitter' | 'gif';

export interface RecycledClip {
	id: string;
	name: string;
	original: string;
	deleted_ms: number;
	size: number;
	duration: number;
	thumb: string | null;
}

export interface LibraryMeta {
	/** Tag -> CSS colour. */
	tag_colors: Record<string, string>;
	/** Session key -> name. */
	sessions: Record<string, string>;
}

export interface Monitor {
	index: number;
	name: string;
	width: number;
	height: number;
	primary: boolean;
}

export interface EncoderInfo {
	kind: 'nvenc' | 'amf' | 'qsv' | 'other' | 'software';
	name: string;
	codec: Codec;
}

export interface BenchResult extends EncoderInfo {
	fps: number;
	ok: boolean;
	error: string | null;
}

export interface Device {
	id: string;
	name: string;
}

export const api = {
	getSettings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Settings) => invoke<void>('set_settings', { settings }),
	defaultSettings: () => invoke<Settings>('default_settings'),
	getStatus: () => invoke<Status>('get_status'),
	monitors: () => invoke<Monitor[]>('list_monitors'),
	encoders: () => invoke<EncoderInfo[]>('list_encoders'),
	microphones: () => invoke<Device[]>('list_microphones'),
	benchmark: (secs: number) => invoke<BenchResult[]>('run_benchmark', { secs }),
	clips: () => invoke<Clip[]>('list_clips'),
	renameClip: (path: string, name: string) => invoke<Clip>('rename_clip', { path, name }),
	deleteClips: (paths: string[]) => invoke<number>('delete_clips', { paths }),
	recycled: () => invoke<RecycledClip[]>('list_recycled'),
	restoreClips: (ids: string[]) => invoke<number>('restore_clips', { ids }),
	setFavorite: (path: string, favorite: boolean) => invoke<Clip>('set_favorite', { path, favorite }),
	setTags: (path: string, tags: string[]) => invoke<Clip>('set_tags', { path, tags }),
	setTagColor: (tag: string, color: string) => invoke<void>('set_tag_color', { tag, color }),
	libraryMeta: () => invoke<LibraryMeta>('library_meta'),
	renameSession: (key: string, name: string) => invoke<void>('rename_session', { key, name }),
	clipsDir: () => invoke<string>('clips_dir'),
	saveNow: (secs?: number) => invoke<void>('save_clip_now', { secs: secs ?? null }),
	saveMarker: (id: number, secs: number) => invoke<void>('save_marker', { id, secs }),
	screenshot: () => invoke<void>('take_screenshot'),
	trim: (path: string, from: number, to: number) => invoke<Clip>('trim_clip', { path, from, to }),
	merge: (paths: string[]) => invoke<Clip>('merge_clips', { paths }),
	exportClip: (path: string, preset: ExportPreset) => invoke<string>('export_clip', { path, preset }),
	setThumbnail: (path: string, secs: number) => invoke<Clip>('set_thumbnail', { path, secs }),
	keyframes: (path: string) => invoke<[number[], number]>('clip_keyframes', { path }),
	exportSettings: (path: string) => invoke<void>('export_settings', { path }),
	importSettings: (path: string) => invoke<Settings>('import_settings', { path }),
	runningApps: () => invoke<string[]>('running_apps')
};

export const fileUrl = (path: string) => convertFileSrc(path);

export function fmtDuration(secs: number): string {
	const s = Math.max(0, Math.round(secs));
	const m = Math.floor(s / 60);
	return `${m}:${String(s % 60).padStart(2, '0')}`;
}

export function fmtBytes(n: number): string {
	if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
	if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
	return `${Math.max(1, Math.round(n / 1e3))} KB`;
}

export function fmtWhen(ms: number): string {
	const d = new Date(ms);
	const now = new Date();
	const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	if (d.toDateString() === now.toDateString()) return `Today, ${time}`;
	const y = new Date(now);
	y.setDate(now.getDate() - 1);
	if (d.toDateString() === y.toDateString()) return `Yesterday, ${time}`;
	return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })}, ${time}`;
}

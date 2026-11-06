import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { fmt } from './format.svelte.ts';

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

/** Settings that switch in while a game runs; null keeps the global value. */
export interface GameProfile {
	exe: string;
	name: string;
	enabled: boolean;
	fps: number | null;
	height: number | null;
	bitrate_kbps: number | null;
	codec: Codec | null;
	/** Slot A. */
	buffer_seconds: number | null;
	slot_b_seconds: number | null;
	slot_c_seconds: number | null;
	hotkeys: Hotkeys | null;
	/** Save into `<clips folder>/<name>/`. */
	subfolder: boolean;
	keystroke_preset: string | null;
}

export interface Hotkeys {
	save_clip: string;
	marker: string;
	screenshot: string;
	extend_last: string;
	slow_mo: string;
	favorite_last: string;
	compact_hud: string;
}

export interface BufferSlot {
	enabled: boolean;
	seconds: number;
	hotkey: string;
}

export interface DiscordWebhook {
	name: string;
	url: string;
	/** Upload cap in MB: 10, or 25 for a boosted server. */
	limit_mb: 10 | 25;
}

export type Trigger =
	| { kind: 'game_detected' | 'idle' | 'on_battery' | 'plugged_in' }
	| { kind: 'app_focused'; exe: string }
	| { kind: 'time_of_day'; from: string; to: string };

/** Settings that switch in by circumstance; null/false keeps the usual value. */
export interface SituationalProfile {
	name: string;
	enabled: boolean;
	trigger: Trigger;
	fps: number | null;
	height: number | null;
	bitrate_kbps: number | null;
	codec: Codec | null;
	buffer_seconds: number | null;
	hotkeys: Hotkeys | null;
	overlays_off: boolean;
}

export type MacroStep =
	| { kind: 'save_clip' | 'set_marker' | 'screenshot' | 'slow_mo' | 'extend_last' | 'open_last_clip' | 'toggle_hud' | 'pause_buffer' }
	| { kind: 'run_export'; preset: ExportPreset }
	| { kind: 'post_discord'; webhook: number }
	| { kind: 'show_toast'; message: string };
export type TrayItemId = 'open' | 'save' | 'marker' | 'screenshot' | 'library' | 'settings' | 'folder' | 'pause' | 'quit';
export type ToastPosition = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right' | 'top_center' | 'bottom_center';
export type NoticeEvent = 'clip_saved' | 'marker' | 'screenshot' | 'update' | 'benchmark';
export type LibrarySort = 'newest' | 'oldest' | 'name' | 'duration' | 'game' | 'size';
export const SETTINGS_SECTIONS = ['essentials', 'hotkeys', 'macros', 'appearance', 'notifications', 'capture', 'performance', 'library', 'storage', 'sharing', 'games', 'help', 'advanced'] as const;
export type FpsMode = 'fixed' | 'display';
export type DashboardWidgetId = 'save' | 'buffer' | 'recent' | 'disk' | 'counters' | 'status' | 'playtime';

export interface Settings {
	version: number;
	first_run_complete: boolean;
	/** The Live tab's four-step tour has been finished or skipped. */
	tour_complete: boolean;
	pro_mode: boolean;
	/** 'system' or a theme id from themes.ts. */
	theme: string;
	clips_dir: string;
	launch_on_startup: boolean;
	start_minimized: boolean;
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
	/** Opt-in: a normal save also records `seconds` (1-120) after the keypress. */
	post_roll: { enabled: boolean; seconds: number };
	slow_mo: { seconds: 2 | 3 | 5; speed: number };
	playback: { fullscreen_on_open: boolean };
	/** Settings sections in the user's order (ids as in the page). */
	section_order: string[];
	/** Circumstance-based profiles, first match wins. Empty = nothing runs. */
	situational: SituationalProfile[];
	/** Custom hotkey chains, up to ten. */
	macros: { name: string; hotkey: string; enabled: boolean; steps: MacroStep[] }[];
	tray: {
		/** Menu entries in order; quit is always on. */
		items: { id: TrayItemId; enabled: boolean }[];
		left_click: 'open_window' | 'save_clip' | 'compact_hud';
	};
	notifications: {
		position: ToastPosition;
		/** Seconds a toast stays, 1-10. */
		duration_secs: number;
		/** Which events show a toast; warnings and errors always do. */
		events: Record<NoticeEvent, boolean>;
		/** `none`, a built-in id (blip, chime, click, pop, low) or `file:<path>.wav`. */
		sounds: Record<NoticeEvent, string>;
	};
	appearance: {
		/** `#rrggbb` over the theme's accent; '' = the theme's own. */
		accent: string;
		font: 'theme' | 'inter' | 'mono' | 'system';
		/** 80, 90, 100, 110, 125 or 150. */
		ui_scale: number;
		/** Encoder / FPS / game / buffer pills on the dashboard. */
		status_pills: boolean;
		time_format: 'system' | '12h' | '24h';
		date_format: 'friendly' | 'iso' | 'dmy' | 'mdy' | 'relative';
		duration_format: 'clock' | 'words' | 'seconds';
		size_format: 'si' | 'iec';
		language: 'en';
	};
	/** Clip file name template; tokens in naming.ts. */
	naming: { clip_template: string; folder_mode: 'flat' | 'game' | 'date' | 'game_date' | 'custom'; folder_template: string };
	library: {
		session_gap_mins: number;
		purge_days: number;
		/** Smallest tile width, px (120-400). */
		tile_size: number;
		density: 'compact' | 'comfortable' | 'spacious';
		layout: 'grid' | 'list';
		sort: LibrarySort;
		thumb_style: 'square' | 'wide' | 'original';
		meta: { duration: boolean; date: boolean; game: boolean; tags: boolean; size: boolean };
	};
	audio: AudioTrack[];
	hotkeys: Hotkeys;
	/** Off: profiles are never applied. */
	profiles_enabled: boolean;
	profiles: GameProfile[];
	privacy_apps: string[];
	ssd_warn_gb_per_day: number;
	sharing: {
		discord_webhooks: DiscordWebhook[];
		/** Listen for clips from other Rewinds and announce this PC. */
		lan_enabled: boolean;
		lan_port: number;
		/** Name other PCs see; empty = this PC's name. */
		device_name: string;
		device_id: string;
		/** PCs whose clips are accepted without asking. */
		trusted_peers: { id: string; name: string }[];
		/** Show "Playing <game> with Rewind" on Discord. Off: no connection is made. */
		discord_presence: boolean;
	};
	watch_folders: WatchFolder[];
	/** Compact HUD pill instead of the main window. Position in physical pixels. */
	hud: { enabled: boolean; x: number | null; y: number | null };
	/** Clips-drive free space limits, GB: warn below one, refuse saves below the other. */
	disk: { warn_gb: number; block_gb: number };
	smart_highlights: SmartHighlights;
	keystrokes: Keystrokes;
	/** Monitor index the on-screen overlays go on; null = the recorded monitor. */
	overlay_monitor: number | null;
	performance: {
		/** 720p30 HEVC ~4 Mb/s, 30 s buffer, overlays off, laid over your settings. */
		low_spec: boolean;
		/** Use the low preset while on battery. */
		battery_throttle: boolean;
		fps_mode: FpsMode;
	};
	/** Dashboard widgets in display order; 'save' is always enabled. */
	dashboard: { id: DashboardWidgetId; enabled: boolean }[];
}

export type KeyFilterMode = 'all' | 'allow' | 'block';
export type KeycapStyle = 'flat' | 'three_d' | 'minimal' | 'outlined' | 'mechanical';
export type OverlayFont = 'theme' | 'mono' | 'sans' | 'condensed';
export type PressAnimation = 'none' | 'pop' | 'flash' | 'spring' | 'pulse';
export type OverlayAnchor = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right' | 'top_center' | 'bottom_center' | 'custom';

/** Keystroke overlay. Off: no input hook is installed and nothing is recorded. */
export interface Keystrokes {
	enabled: boolean;
	/** Also draw input on screen as it happens. Shares style, position and filters. */
	live: boolean;
	keyboard: boolean;
	clicks: boolean;
	/** ~60 positions a second; large files. */
	movement: boolean;
	scroll: boolean;
	filter: KeyFilterMode;
	/** Key names for the allow or block list. */
	keys: string[];
	preset: string;
	pause_in_text_fields: boolean;
	style: {
		keycap: KeycapStyle;
		/** Keycap height in px at 1080p, 16-120. */
		size: number;
		font: OverlayFont;
		/** `#rrggbb`; empty = default. */
		base_color: string;
		/** `#rrggbb`; empty = automatic contrast. */
		text_color: string;
		border: boolean;
		border_color: string;
		radius: number;
		shadow: boolean;
		shadow_intensity: number;
		animation: PressAnimation;
		fade_secs: number;
		idle_opacity: number;
		pressed_opacity: number;
		hold_indicator: boolean;
		combo: 'chain' | 'stack';
	};
	position: {
		anchor: OverlayAnchor;
		/** Custom top-left, 0-1 of the frame. */
		x: number;
		y: number;
		margin: number;
		click_position: 'cursor' | 'with_keys';
		click_style: 'dot' | 'ring' | 'flash';
		trail: boolean;
		trail_ms: number;
	};
}

export interface MontageRequest {
	/** In playing order. */
	paths: string[];
	transition: 'cut' | 'crossfade';
	/** Longest piece per clip, seconds; null = whole clips. */
	max_secs: number | null;
	music: string | null;
	clip_audio: boolean;
}

export type StorageKind = 'clips' | 'screenshots' | 'exports' | 'recycled' | 'thumbnails' | 'app_data';

/** Disk use of Rewind's files by kind, measured on request. */
export interface StorageBreakdown {
	categories: { kind: StorageKind; bytes: number; files: number }[];
	total: number;
	drive_total: number | null;
	drive_free: number | null;
}

export interface KeystrokePreset {
	id: string;
	label: string;
	filter: KeyFilterMode;
	keys: string[];
	keyboard: boolean;
	clicks: boolean;
	scroll: boolean;
}

/** Audio-based smart highlights. Off: nothing is ever analysed. */
export interface SmartHighlights {
	enabled: boolean;
	/** dBFS, -30 to -3. */
	threshold_db: number;
	/** Also analyse each clip right after it's saved. */
	analyze_on_save: boolean;
}

/** A flagged moment in a clip, seconds from its start. */
export interface Moment {
	at: number;
	end: number;
	/** 0-1. */
	score: number;
	reason: 'spike' | 'voice' | 'sustained';
}

/** New .mp4 files in `input` are exported with `preset` into `output`. */
export interface WatchFolder {
	enabled: boolean;
	input: string;
	preset: ExportPreset;
	output: string;
	delete_original: boolean;
}

export interface WatchOp {
	at_ms: number;
	input: string;
	output: string | null;
	preset: ExportPreset;
	error: string | null;
	deleted: boolean;
}

export interface LanPeer {
	id: string;
	name: string;
	addr: string;
}

/** Progress of a LAN transfer, about once per 20 MB chunk. */
export interface LanProgress {
	direction: 'in' | 'out';
	peer: string;
	file: string;
	/** Sending only. */
	path?: string;
	done: number;
	total: number;
}

export interface LanOffer {
	offer: number;
	name: string;
	file: string;
	size: number;
}

export type HotkeyAction = 'save_clip' | 'marker' | 'screenshot' | 'extend_last' | 'slow_mo' | 'favorite_last' | 'toggle_hud' | 'toggle_pause';

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
	/** Game profile in effect. */
	profile: string | null;
	privacy: boolean;
	/** Paused by hand (tray or macro). */
	manual_pause: boolean;
	/** Situational profile in force. */
	situation: string | null;
	/** Markers set this session, oldest first. */
	markers: MarkerInfo[];
	/** A save still recording its post-roll. */
	post_roll: PostRollStatus | null;
	error: string | null;
	written_today_gb: number;
	/** Since Monday. */
	written_week_gb: number;
	/** Free space on the clips drive, null until measured. */
	disk_free_gb: number | null;
	/** `full`: new clip saves are refused (screenshots still work). */
	disk: 'ok' | 'low' | 'full';
	/** Captured audio loudness (RMS 0-1) in 100 ms steps since the last status, oldest first. */
	levels: number[];
}

export interface PostRollStatus {
	/** Seconds until the current save is written. */
	remaining: number;
	seconds: number;
	/** Further presses waiting behind it. */
	queued: number;
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
	/** Smart-highlight moments; null until analysed. */
	highlights: Moment[] | null;
	/** Keystrokes were recorded with this clip. */
	has_input: boolean;
}

/** A PNG saved by the screenshot hotkey. */
export interface Screenshot {
	path: string;
	name: string;
	size: number;
	modified: number;
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
	/** Top-left on the virtual desktop, physical pixels. */
	x: number;
	y: number;
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

/** Settings benchmark: the live pipeline measured over a few seconds. */
export interface BenchReport {
	secs: number;
	encoder: string;
	width: number;
	height: number;
	target_fps: number;
	capture_fps: number;
	encoded_fps: number;
	/** Highest rate the encoder could keep up, from how busy it was. */
	encoder_max_fps: number;
	/** 0-1. */
	encoder_load: number;
	dropped: number;
	bitrate_kbps: number;
	cpu_pct: number;
	rewind_cpu_pct: number;
	gpu_pct: number | null;
	gpu_encode_pct: number | null;
	ram_mb: number;
	buffer_mb: number;
	verdict: { grade: 'pass' | 'warn' | 'fail'; headline: string; detail: string[] };
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
	screenshots: () => invoke<Screenshot[]>('list_screenshots'),
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
	/** Do what a hotkey does; errors arrive as notices. */
	runAction: (action: HotkeyAction) => invoke<void>('run_action', { action }),
	trim: (path: string, from: number, to: number) => invoke<Clip>('trim_clip', { path, from, to }),
	merge: (paths: string[]) => invoke<Clip>('merge_clips', { paths }),
	/** `overlay`: draw recorded keystrokes in, when the clip has any. */
	exportClip: (path: string, preset: ExportPreset, overlay = false) => invoke<string>('export_clip', { path, preset, overlay }),
	keystrokePresets: () => invoke<KeystrokePreset[]>('keystroke_presets'),
	/** Resolves with the new clip. Progress (0-1): `montage-progress` events. */
	makeMontage: (request: MontageRequest) => invoke<Clip>('make_montage', { request }),
	storageBreakdown: () => invoke<StorageBreakdown>('storage_breakdown'),
	setThumbnail: (path: string, secs: number) => invoke<Clip>('set_thumbnail', { path, secs }),
	keyframes: (path: string) => invoke<[number[], number]>('clip_keyframes', { path }),
	exportSettings: (path: string) => invoke<void>('export_settings', { path }),
	importSettings: (path: string) => invoke<Settings>('import_settings', { path }),
	runningApps: () => invoke<string[]>('running_apps'),
	/** `webhook` is the index into `settings.sharing.discord_webhooks`. */
	postToDiscord: (path: string, webhook: number) => invoke<void>('post_to_discord', { path, webhook }),
	lanPeers: () => invoke<LanPeer[]>('lan_peers'),
	/** Resolves once the clip is saved on the other PC. */
	lanSend: (path: string, peer: string) => invoke<void>('lan_send', { path, peer }),
	lanAnswer: (offer: number, accept: boolean) => invoke<void>('lan_answer', { offer, accept }),
	watchHistory: () => invoke<WatchOp[]>('watch_history'),
	setCompactHud: (on: boolean) => invoke<void>('set_compact_hud', { on }),
	/** Right-click menu on the HUD pill. */
	hudMenu: () => invoke<void>('hud_menu'),
	liveBenchmark: (secs: number) => invoke<BenchReport>('run_live_benchmark', { secs }),
	/** Resolves with the number of moments found. Progress: `analysis-progress` events. */
	analyzeClips: (paths: string[]) => invoke<number>('analyze_clips', { paths }),
	/** Play a notification sound setting once. */
	/** Run macro number `index` now. Errors arrive as notices. */
	runMacro: (index: number) => invoke<void>('run_macro', { index }),
	playSound: (sound: string) => invoke<void>('play_sound', { sound }),
	/** Silence toasts and sounds until Rewind restarts. Never saved. */
	setDoNotDisturb: (on: boolean) => invoke<void>('set_do_not_disturb', { on }),
	doNotDisturb: () => invoke<boolean>('do_not_disturb')
};

export const fileUrl = (path: string) => convertFileSrc(path);

/** A length of time, written as the user chose (1:23, 1m 23s or 83s). */
export function fmtDuration(secs: number): string {
	const s = Math.max(0, Math.round(secs));
	if (fmt.duration === 'seconds') return `${s}s`;
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	if (fmt.duration === 'words') return h ? `${h}h ${m}m ${s % 60}s` : m ? `${m}m ${s % 60}s` : `${s}s`;
	return h ? `${h}:${String(m).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}` : `${m}:${String(s % 60).padStart(2, '0')}`;
}

/** A position in a clip, always m:ss: it's a clock, not a length. */
export function fmtClock(secs: number): string {
	const s = Math.max(0, Math.round(secs));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export function fmtBytes(n: number): string {
	const k = fmt.size === 'iec' ? 1024 : 1000;
	const u = fmt.size === 'iec' ? ['KiB', 'MiB', 'GiB'] : ['KB', 'MB', 'GB'];
	if (n >= k ** 3) return `${(n / k ** 3).toFixed(1)} ${u[2]}`;
	if (n >= k ** 2) return `${(n / k ** 2).toFixed(1)} ${u[1]}`;
	return `${Math.max(1, Math.round(n / k))} ${u[0]}`;
}

const hour12 = () => (fmt.time === 'system' ? undefined : fmt.time === '12h');

/** Time of day, in the chosen 12 or 24 hour style. */
export function fmtTime(ms: number, seconds = false): string {
	return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', ...(seconds ? { second: '2-digit' } : {}), hour12: hour12() });
}

const p2 = (n: number) => String(n).padStart(2, '0');

/** A calendar date in the chosen style (no time). */
export function fmtDate(ms: number): string {
	const d = new Date(ms);
	const now = new Date();
	const days = Math.round((new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() - new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()) / 86_400_000);
	switch (fmt.date) {
		case 'iso':
			return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
		case 'dmy':
			return `${p2(d.getDate())}/${p2(d.getMonth() + 1)}/${d.getFullYear()}`;
		case 'mdy':
			return `${p2(d.getMonth() + 1)}/${p2(d.getDate())}/${d.getFullYear()}`;
		case 'relative': {
			if (days <= 0) return 'Today';
			if (days === 1) return 'Yesterday';
			if (days < 14) return `${days} days ago`;
			if (days < 60) return `${Math.floor(days / 7)} weeks ago`;
			if (days < 365) return `${Math.floor(days / 30)} months ago`;
			const y = Math.floor(days / 365);
			return `${y} ${y === 1 ? 'year' : 'years'} ago`;
		}
		default:
			if (days <= 0) return 'Today';
			if (days === 1) return 'Yesterday';
			return d.toLocaleDateString([], { month: 'short', day: 'numeric', ...(d.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }) });
	}
}

/** Date and time: "Today, 14:03", "2026-11-05, 14:03", "5 days ago". */
export function fmtWhen(ms: number): string {
	const date = fmtDate(ms);
	const today = date === 'Today';
	// "5 days ago" already says when; a clock time next to it adds nothing.
	if (fmt.date === 'relative' && !today) return date;
	return `${date}, ${fmtTime(ms)}`;
}

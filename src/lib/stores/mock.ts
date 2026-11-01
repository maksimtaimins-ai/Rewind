// Sample data for previewing the UI in a plain browser (no Tauri backend).
import type { Clip, Settings, Status } from '#lib/api.ts';

export function mockSettings(): Settings {
	return {
		version: 17,
		first_run_complete: true,
		tour_complete: false,
		pro_mode: false,
		theme: 'system',
		clips_dir: 'C:\\Users\\you\\AppData\\Roaming\\Rewind\\Clips',
		launch_on_startup: false,
		start_minimized: false,
		play_sound_on_save: true,
		video: {
			monitor: 0,
			height: 1080,
			fps: 60,
			codec: 'h264',
			encoder: 'auto',
			rate_control: 'vbr',
			bitrate_kbps: 20000,
			quality: 70,
			keyframe_secs: 1,
			capture_cursor: true,
			hdr: 'tonemap_to_sdr'
		},
		buffer: {
			seconds: 30,
			extra_slots: [
				{ enabled: true, seconds: 120, hotkey: 'Alt+F9' },
				{ enabled: false, seconds: 300, hotkey: '' }
			],
			max_memory_mb: 1024, extend_secs: 15 },
		post_roll: { enabled: false, seconds: 10 },
		slow_mo: { seconds: 3, speed: 0.25 },
		library: { session_gap_mins: 30, purge_days: 30 },
		audio: [
			{ name: 'Desktop', enabled: true, source: { kind: 'system' }, volume: 1 },
			{ name: 'Microphone', enabled: false, source: { kind: 'mic', device_id: null }, volume: 1 },
			{ name: 'Discord', enabled: false, source: { kind: 'app', exe: 'Discord.exe' }, volume: 1 }
		],
		hotkeys: {
			save_clip: 'Alt+F8',
			marker: 'Alt+F7',
			screenshot: 'Alt+F12',
			extend_last: 'Alt+F6',
			slow_mo: 'Alt+F5',
			favorite_last: 'Alt+Shift+F',
			compact_hud: 'Alt+F11'
		},
		profiles: [],
		privacy_apps: ['KeePassXC.exe', '1Password.exe', 'Bitwarden.exe'],
		ssd_warn_gb_per_day: 50,
		sharing: {
			discord_webhooks: [{ name: 'Squad #clips', url: 'https://discord.com/api/webhooks/1/x', limit_mb: 10 }],
			lan_enabled: true,
			lan_port: 7777,
			device_name: '',
			device_id: 'mock',
			trusted_peers: [{ id: 'laptop', name: 'LAPTOP-7Q2' }]
		},
		hud: { enabled: false, x: null, y: null },
		disk: { warn_gb: 10, block_gb: 2 },
		highlight: { color: 'off', custom_color: '#ffc53d', thickness: 2, opacity: 60, position: 'border', only_in_game: false },
		smart_highlights: { enabled: true, threshold_db: -14, analyze_on_save: false },
		watch_folders: [
			{ enabled: true, input: 'C:\\clips\\outbox\\youtube-shorts', preset: 'shorts', output: 'C:\\clips\\exported\\youtube', delete_original: false }
		]
	};
}

export const mockTagColors: Record<string, string> = { ace: '#ffc53d', clutch: '#0090ff', boss: '#e5484d', fail: '#8e4ec6' };

export function mockStatus(): Status {
	return {
		state: 'recording',
		buffered_secs: 24.3,
		buffer_secs: 30,
		slots: [
			{ name: 'A', seconds: 30, held: 24.3, hotkey: 'Alt+F8' },
			{ name: 'B', seconds: 120, held: 24.3, hotkey: 'Alt+F9' }
		],
		fps: 59.9,
		bitrate_kbps: 18420,
		dropped: 0,
		memory_mb: 104.6,
		encoder: 'NVIDIA H.264 Encoder MFT',
		resolution: [2580, 1080],
		game: 'Counter-Strike 2',
		privacy: false,
		markers: [
			{ id: 1, ago: 340, at_ms: Date.now() - 340e3, in_buffer: false },
			{ id: 2, ago: 21.6, at_ms: Date.now() - 21.6e3, in_buffer: true },
			{ id: 3, ago: 8.2, at_ms: Date.now() - 8.2e3, in_buffer: true }
		],
		post_roll: null,
		error: null,
		written_today_gb: 0.31,
		written_week_gb: 31.6,
		disk_free_gb: 7.4,
		disk: 'low',
		levels: []
	};
}

export function mockClips(): Clip[] {
	const now = Date.now();
	const names = [
		'Counter-Strike 2 2026-10-31 21-14-02',
		'Counter-Strike 2 2026-10-31 20-58-41',
		'Rocket League 2026-10-30 23-02-10',
		'Desktop 2026-10-30 18-44-55',
		'Elden Ring 2026-10-29 22-31-07',
		'Elden Ring 2026-10-29 22-12-50'
	];
	return names.map((name, i) => ({
		path: `C:\\clips\\${name}.mp4`,
		name,
		size: 40e6 + i * 13e6,
		modified: now - [0, 9e5, 2.1e6, 9e6, 9.9e6, 4e7][i],
		duration: [60, 60, 35, 12, 60, 90][i],
		thumb: null,
		thumb_rev: 0,
		favorite: i === 1 || i === 4,
		tags: [['ace', 'clutch'], ['clutch'], [], [], ['boss'], ['boss', 'fail']][i],
		highlights: i === 0
			? [
					{ at: 8.4, end: 9.1, score: 0.82, reason: 'spike' as const },
					{ at: 21, end: 26.5, score: 0.64, reason: 'sustained' as const },
					{ at: 41.2, end: 42.3, score: 0.91, reason: 'voice' as const }
				]
			: i === 2
				? []
				: null
	}));
}

/** Five 100 ms loudness steps of made-up game audio: a bed of noise with the odd burst. */
let phase = 0;
export function mockLevels(): number[] {
	return Array.from({ length: 5 }, () => {
		phase += 0.1;
		const burst = Math.max(0, Math.sin(phase * 0.9) * Math.sin(phase * 2.3)) ** 3;
		return Math.min(1, 0.06 + Math.random() * 0.08 + burst * (0.35 + Math.random() * 0.25));
	});
}

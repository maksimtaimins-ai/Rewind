// Quality presets suggested by the first-run benchmark. Mirrors `perf::suggest` in the backend.
import type { Settings } from '#lib/api.ts';

export type Preset = 'low' | 'medium' | 'high';

export const presetLabel: Record<Preset, string> = { low: 'Low', medium: 'Medium', high: 'High' };

export const presetSummary: Record<Preset, string> = {
	low: '720p at 30 fps, about 4 Mb/s, 30 second buffer, nothing extra running',
	medium: '1080p at 60 fps, about 12 Mb/s, 30 second buffer',
	high: '1080p at 60 fps, about 20 Mb/s, 60 second buffer'
};

/** The preset an encoder speed (frames per second at the output size) can carry with headroom. */
export function suggest(encoderFps: number, hardware: boolean): Preset {
	if (!hardware || encoderFps < 90) return 'low';
	return encoderFps < 180 ? 'medium' : 'high';
}

/** Write a preset into settings. Low turns on low-spec mode; the others set plain values and turn it off. */
export function applyPreset(x: Settings, p: Preset, hevc: boolean) {
	x.performance.low_spec = p === 'low';
	if (p === 'low') return;
	x.video.height = 1080;
	x.video.fps = 60;
	x.video.codec = hevc ? 'hevc' : 'h264';
	x.video.bitrate_kbps = p === 'medium' ? 12000 : 20000;
	x.buffer.seconds = p === 'medium' ? 30 : 60;
}

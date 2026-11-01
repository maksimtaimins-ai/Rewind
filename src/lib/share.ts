// Sharing flows that need a prompt or two around the backend call.
import { ask } from '@tauri-apps/plugin-dialog';
import { api, type Clip } from '#lib/api.ts';
import { app, inApp, refreshClips, toast } from '#lib/stores/app.svelte.ts';

/**
 * Post a clip to webhook `index`. A clip over the webhook's cap is offered as
 * a Discord-preset export first, and that copy is posted instead.
 */
export async function postToDiscord(clip: Clip, index: number) {
	const w = app.settings?.sharing.discord_webhooks[index];
	if (!w || !inApp) return;
	let path = clip.path;
	if (clip.size > w.limit_mb * 1e6) {
		const ok = await ask(
			`"${clip.name}" is ${(clip.size / 1e6).toFixed(1)} MB, over the ${w.limit_mb} MB limit for ${w.name}. Export a Discord ${w.limit_mb} MB copy and post that instead?`,
			{ title: 'Too large for Discord', kind: 'warning', okLabel: 'Export and post', cancelLabel: 'Cancel' }
		);
		if (!ok) return;
		try {
			toast('info', `Exporting a ${w.limit_mb} MB copy…`);
			path = await api.exportClip(clip.path, w.limit_mb >= 25 ? 'discord25' : 'discord10');
			await refreshClips();
		} catch (e) {
			toast('error', `Export failed: ${e}`);
			return;
		}
	}
	try {
		await api.postToDiscord(path, index);
		toast('success', `Posted to ${w.name}`);
	} catch (e) {
		toast('error', `Couldn't post to ${w.name}: ${e}`);
	}
}

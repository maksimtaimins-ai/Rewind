<!--
  Settings → Notifications: where toasts appear, how long they stay, which
  events show one, and an optional sound per event. Do not disturb mutes
  everything until the next launch and is never saved.
-->
<script lang="ts">
	import { open } from '@tauri-apps/plugin-dialog';
	import { api, type NoticeEvent, type Settings, type ToastPosition } from '#lib/api.ts';
	import { app, inApp, setDnd, toast, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from './Row.svelte';
	import Toggle from './Toggle.svelte';

	const n = $derived(app.settings!.notifications);
	const set = (f: (n: Settings['notifications']) => void) => updateSettings((x) => f(x.notifications));

	const spots: { value: ToastPosition; label: string }[] = [
		{ value: 'top_left', label: 'Top left' },
		{ value: 'top_center', label: 'Top centre' },
		{ value: 'top_right', label: 'Top right' },
		{ value: 'bottom_left', label: 'Bottom left' },
		{ value: 'bottom_center', label: 'Bottom centre' },
		{ value: 'bottom_right', label: 'Bottom right' }
	];

	const events: { id: NoticeEvent; label: string; hint: string }[] = [
		{ id: 'clip_saved', label: 'Clip saved', hint: 'When a clip, slow-mo or extended clip is written.' },
		{ id: 'marker', label: 'Marker set', hint: 'When you mark a moment.' },
		{ id: 'screenshot', label: 'Screenshot', hint: 'When a screenshot is saved.' },
		{ id: 'update', label: 'Update available', hint: 'When "Check for updates" finds a new version.' },
		{ id: 'benchmark', label: 'Encoder benchmarked', hint: 'When a benchmark finishes.' }
	];
	const builtins = [
		['blip', 'Blip'],
		['chime', 'Chime'],
		['click', 'Click'],
		['pop', 'Pop'],
		['low', 'Low thump']
	];

	const fileName = (s: string) => s.slice(5).split(/[\\/]/).pop();

	async function choose(id: NoticeEvent, value: string) {
		if (value !== '__pick') {
			set((x) => (x.sounds[id] = value));
			if (value !== 'none' && inApp) api.playSound(value).catch(() => {});
			return;
		}
		if (!inApp) return;
		const path = await open({ filters: [{ name: 'WAV sound', extensions: ['wav'] }], title: 'Pick a sound' });
		if (typeof path === 'string') {
			set((x) => (x.sounds[id] = `file:${path}`));
			api.playSound(`file:${path}`).catch((e) => toast('error', String(e)));
		}
	}
</script>

<Row id="s-dnd" label="Do not disturb" hint={app.dnd ? 'On. No toasts or sounds until you turn this off or restart Rewind.' : 'Silences every toast and sound until the next launch. Clips still save.'}>
	<Toggle checked={app.dnd} label="Do not disturb" onchange={(v) => setDnd(v)} />
</Row>
<Row id="s-toast-position" label="Position" hint="Which corner or edge of the window toasts appear at.">
	<select value={n.position} onchange={(e) => set((x) => (x.position = e.currentTarget.value as ToastPosition))} aria-label="Toast position">
		{#each spots as o (o.value)}
			<option value={o.value}>{o.label}</option>
		{/each}
	</select>
</Row>
<Row id="s-toast-duration" label="Stay for" hint="Errors stay at least 8 seconds.">
	<input type="range" min="1" max="10" step="1" value={n.duration_secs} onchange={(e) => set((x) => (x.duration_secs = Number(e.currentTarget.value)))} aria-label="Toast duration in seconds" />
	<span class="readout num">{n.duration_secs}s</span>
</Row>

<h3 id="notification-events">Events</h3>
<p class="intro">Each event can show a toast, play a sound, both or neither. Warnings and errors always show.</p>
{#each events as ev (ev.id)}
	{@const snd = n.sounds[ev.id]}
	<Row id="s-notify-{ev.id}" label={ev.label} hint={ev.hint}>
		<select value={snd} onchange={(e) => choose(ev.id, e.currentTarget.value)} aria-label="Sound for {ev.label}">
			<option value="none">No sound</option>
			{#each builtins as [id, label] (id)}
				<option value={id}>{label}</option>
			{/each}
			{#if snd.startsWith('file:')}<option value={snd}>{fileName(snd)}</option>{/if}
			<option value="__pick">Choose a .wav file…</option>
		</select>
		<button class="play" disabled={snd === 'none' || !inApp} aria-label="Play {ev.label} sound" title="Play" onclick={() => api.playSound(snd).catch((e) => toast('error', String(e)))}>
			<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10.5-6.5z" /></svg>
		</button>
		<Toggle checked={n.events[ev.id]} label="Show a toast for {ev.label}" onchange={(v) => set((x) => (x.events[ev.id] = v))} />
	</Row>
{/each}

<style>
	select {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
		max-width: 180px;
	}
	input[type='range'] {
		width: 160px;
		accent-color: var(--text);
	}
	.readout {
		width: 40px;
		text-align: right;
	}
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 4px;
		max-width: 60ch;
	}
	h3 {
		margin: 24px 0 4px;
		font-size: 13px;
		font-weight: 600;
		color: var(--muted);
	}
	.play {
		width: 32px;
		height: 32px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		color: var(--muted);
		cursor: pointer;
	}
	.play:hover:not(:disabled) {
		color: var(--text);
	}
	.play:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.play svg {
		width: 14px;
		height: 14px;
		fill: currentColor;
	}
</style>

<!--
  Settings > Advanced > Performance: every optional feature, whether it's on,
  and what it costs. The second line under each name is read from what's
  actually running (hooks, windows, jobs), not from the toggle.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { app, inApp } from '#lib/stores/app.svelte.ts';

	interface Diagnostics {
		analysis_jobs: number;
		input_hook: boolean;
		profile: string | null;
		montage_jobs: number;
		lan_listening: boolean;
		hud_window: boolean;
		live_keys_window: boolean;
	}

	const s = $derived(app.settings!);
	let d = $state<Diagnostics | null>(null);

	async function check() {
		d = inApp
			? await invoke<Diagnostics>('feature_diagnostics').catch(() => null)
			: { analysis_jobs: 0, input_hook: s.keystrokes.enabled || s.keystrokes.live, profile: s.profiles_enabled ? (app.status?.profile ?? null) : null, montage_jobs: 0, lan_listening: s.sharing.lan_enabled, hud_window: false, live_keys_window: false };
	}
	// Only while this page is open: a cheap read of in-memory flags.
	onMount(() => {
		check();
		const t = setInterval(check, 4000);
		return () => clearInterval(t);
	});
	$effect(() => {
		void app.settings;
		check();
	});

	const mbFor = (secs: number) => Math.round(((s.video.bitrate_kbps * 1000) / 8 / 1e6) * secs);

	interface Item {
		name: string;
		on: boolean;
		/** What it costs while on. */
		cost: string;
		/** Live state, from the running app. */
		live: string;
		/** Costs nothing until used, even when "on" (user-triggered tools). */
		onDemand?: boolean;
		/** Not free when off (a pre-existing background check). */
		offNote?: string;
	}

	const items = $derived<Item[]>([
		{
			name: 'Smart highlights',
			on: s.smart_highlights.enabled,
			cost: s.smart_highlights.analyze_on_save
				? 'About 1 s of one CPU core per minute of clip, right after each save'
				: 'Only when you press Analyze: about 1 s of one CPU core per minute of clip',
			live: d?.analysis_jobs ? 'Analyzing a clip now' : 'Not analyzing'
		},
		{
			name: 'Keystroke overlay',
			on: s.keystrokes.enabled && (s.keystrokes.keyboard || s.keystrokes.clicks || s.keystrokes.movement || s.keystrokes.scroll),
			cost: s.keystrokes.movement
				? 'Under 0.1% CPU; about 1 to 3 MB per minute of clip with pointer movement'
				: 'Under 0.1% CPU, about 1 MB of RAM, 10 to 50 KB per clip',
			live: d?.input_hook ? 'Keyboard/mouse hook installed' : 'No input hook installed'
		},
		{
			name: 'Live keystroke overlay',
			on: s.keystrokes.live && (s.keystrokes.keyboard || s.keystrokes.clicks),
			cost: 'The keyboard/mouse hook (~0% CPU) plus a transparent overlay window: about 30 to 60 MB of RAM, no effect on capture',
			live: d?.live_keys_window ? 'Overlay window open' : 'No overlay window'
		},
		{
			name: 'Game profiles',
			on: s.profiles_enabled,
			cost: 'No extra polling. Capture restarts for about a second when a game with a profile starts or quits',
			live: d?.profile ? `Using the ${d.profile} profile` : 'No profile in use'
		},
		{
			name: 'Montage builder',
			on: false,
			onDemand: true,
			cost: '',
			live: d?.montage_jobs ? 'Making a montage now: about one CPU core until it finishes' : 'Runs only when you make one'
		},
		{
			name: 'Storage bar',
			on: false,
			onDemand: true,
			cost: '',
			live: 'Reads file sizes when you open Settings or Live; nothing in the background'
		},
		{
			name: 'LAN send',
			on: s.sharing.lan_enabled,
			cost: 'An open port and network announcements: about 2 MB of RAM, idle CPU',
			live: d?.lan_listening ? 'Listening for other PCs' : 'Not listening'
		},
		{
			name: 'Watch folders',
			on: s.watch_folders.some((w) => w.enabled),
			cost: 'Lists each folder every 3 s and exports new clips as they appear',
			live: `${s.watch_folders.filter((w) => w.enabled).length} of ${s.watch_folders.length} folders watched`,
			offNote: 'Near zero: a background check still wakes every 3 s and finds nothing to do'
		},
		{
			name: 'Compact HUD',
			on: s.hud.enabled,
			cost: 'A small always-on-top window: about 30 to 60 MB of RAM',
			live: d?.hud_window ? 'HUD window open' : 'No HUD window'
		},
		{
			name: 'Record after press',
			on: s.post_roll.enabled,
			cost: `Keeps ${s.post_roll.seconds}s more in RAM, about ${mbFor(s.post_roll.seconds)} MB at your bitrate`,
			live: app.status?.post_roll ? 'A save is recording its extra seconds' : 'Nothing waiting'
		}
	]);
	const onCount = $derived(items.filter((i) => i.on).length);
</script>

<p class="intro">
	<span class="num">{onCount}</span> of {items.filter((i) => !i.onDemand).length} optional features on. Anything off starts nothing: no hook, thread, window or timer exists until you turn it on.
</p>
<ul class="perf">
	{#each items as it (it.name)}
		<li>
			<div class="who">
				<span class="name">{it.name}</span>
				<span class="live"><span class="dot" class:active={it.on || /now/.test(it.live)}></span>{it.live}</span>
			</div>
			<span class="state num">{it.onDemand ? 'On use' : it.on ? 'On' : 'Off'}</span>
			<div class="cost">
				{#if it.on}
					<span class="text">{it.cost}</span>
				{:else if it.offNote}
					<span class="badge near">Near zero</span>
					<span class="text sub">{it.offNote}</span>
				{:else}
					<span class="badge free">No cost</span>
				{/if}
			</div>
		</li>
	{/each}
</ul>
<button class="link" onclick={check}>Check again</button>

<style>
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 8px;
		max-width: 64ch;
	}
	.perf {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: grid;
		grid-template-columns: minmax(0, 1.1fr) 56px minmax(0, 1.4fr);
		align-items: center;
		gap: 16px;
		padding: 10px 0;
		box-shadow: 0 1px 0 var(--line);
	}
	.who {
		display: grid;
		gap: 2px;
		min-width: 0;
	}
	.name {
		font-weight: 500;
	}
	.live {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--faint);
		font-size: 12px;
	}
	.dot {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		box-shadow: inset 0 0 0 1.5px var(--faint);
	}
	.dot.active {
		background: var(--text);
		box-shadow: none;
	}
	.state {
		color: var(--muted);
		font-size: 11px;
	}
	.cost {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 8px;
		font-size: 12px;
	}
	.text {
		color: var(--text);
	}
	.text.sub {
		color: var(--faint);
	}
	.badge {
		padding: 2px 8px;
		border-radius: 10px;
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
	}
	/* Green reads as "free" in every theme; light themes get a darker one. */
	.free {
		color: #3fb950;
		background: color-mix(in srgb, #3fb950 14%, transparent);
	}
	:global(html[data-scheme='light']) .free {
		color: #1a7f37;
		background: color-mix(in srgb, #1a7f37 12%, transparent);
	}
	.near {
		color: var(--muted);
		background: var(--raised);
	}
	.link {
		margin-top: 8px;
		border: 0;
		background: none;
		padding: 0;
		color: var(--muted);
		cursor: pointer;
	}
	.link:hover {
		color: var(--text);
	}
</style>

<!-- Live keystroke overlay: recent input drawn as it happens in a transparent,
     click-through window over the primary display, with the same layout and
     style as exports. The backend creates the window and sends the events;
     this only draws. Frames run only while something is fading. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { app, inApp } from '#lib/stores/app.svelte.ts';
	import { sceneAt, type PEvent, type PScene } from '#lib/overlay.ts';
	import OverlayScene from '#lib/components/OverlayScene.svelte';

	interface LiveEvent {
		type: 'key_down' | 'key_up' | 'mouse_down' | 'mouse_up';
		key: string;
		mods: string[];
		/** 0-1 of this window's monitor, for clicks. */
		x: number | null;
		y: number | null;
	}

	const KINDS = { key_down: 'down', key_up: 'up', mouse_down: 'mouse_down', mouse_up: 'mouse_up' } as const;
	/** A key held longer than this without a release is treated as stuck. */
	const STUCK_SECS = 60;

	const cfg = $derived(app.settings!.keystrokes);
	let w = $state(innerWidth);
	let h = $state(innerHeight);
	let scene = $state<PScene>({ unit: 0, caps: [], pluses: [], clicks: [] });

	// Not reactive: frames read it directly.
	let events: PEvent[] = [];
	let raf = 0;

	const now = () => performance.now() / 1000;

	/** Drop presses that have finished fading; keep held ones. */
	function prune(t: number) {
		// Click markers last 0.5 s after release.
		const cutoff = t - Math.max(cfg.style.fade_secs, 0.5) - 0.1;
		const keep: (PEvent | null)[] = [];
		const open = new Map<string, number>();
		for (const e of events) {
			if (e.kind === 'down' || e.kind === 'mouse_down') {
				const prev = open.get(e.key);
				if (prev !== undefined) keep[prev] = null;
				open.set(e.key, keep.length);
				keep.push(e);
				continue;
			}
			const i = open.get(e.key);
			if (i === undefined) continue;
			open.delete(e.key);
			if (e.t < cutoff) keep[i] = null;
			else keep.push(e);
		}
		for (const i of open.values()) if (t - keep[i]!.t > STUCK_SECS) keep[i] = null;
		events = keep.filter((e): e is PEvent => e !== null);
	}

	function frame() {
		raf = 0;
		const t = now();
		prune(t);
		scene = sceneAt(events, t, cfg, w, h);
		if (events.length) raf = requestAnimationFrame(frame);
	}
	const kick = () => (raf ||= requestAnimationFrame(frame));

	// Restyle or reposition as soon as settings change.
	$effect(() => {
		void cfg;
		void w;
		void h;
		kick();
	});

	onMount(() => {
		if (!inApp) return;
		const off = listen<LiveEvent>('live-input', ({ payload: e }) => {
			const onScreen = e.x != null && e.y != null && e.x >= 0 && e.x <= 1 && e.y >= 0 && e.y <= 1;
			events.push({ t: now(), kind: KINDS[e.type], key: e.key, mods: e.mods, x: onScreen ? e.x! : undefined, y: onScreen ? e.y! : undefined });
			kick();
		});
		return () => {
			off.then((f) => f());
			cancelAnimationFrame(raf);
		};
	});
</script>

<svelte:head><title>Rewind live keystrokes</title></svelte:head>
<svelte:window onresize={() => ((w = innerWidth), (h = innerHeight))} />

<div class="screen" aria-hidden="true">
	<OverlayScene {scene} {cfg} {h} />
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: hidden;
		pointer-events: none;
	}
	.screen {
		position: fixed;
		inset: 0;
	}
</style>

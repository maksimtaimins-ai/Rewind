<!--
  Live preview of the keystroke overlay on a 16:9 stand-in frame, laid out by
  the same math exports use (overlay.ts). Drag it to place it anywhere; that
  switches the position to Custom.
-->
<script lang="ts">
	import type { Keystrokes } from '#lib/api.ts';
	import { bounds, demo, sceneAt, type PScene } from '#lib/overlay.ts';
	import OverlayScene from './OverlayScene.svelte';

	let { cfg, onmove }: { cfg: Keystrokes; onmove: (x: number, y: number) => void } = $props();

	let w = $state(0);
	const h = $derived(Math.round((w * 9) / 16));
	const script = $derived(demo(cfg));
	const reduced = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
	let t = $state(0.6);

	$effect(() => {
		if (reduced) return;
		let raf = 0;
		const start = performance.now();
		const tick = (now: number) => {
			t = ((now - start) / 1000) % script.length;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(raf);
	});

	// While dragging, a local position stands in for the saved one.
	let drag = $state<{ x: number; y: number; px: number; py: number; ox: number; oy: number } | null>(null);
	const live = $derived(drag ? { ...cfg, position: { ...cfg.position, anchor: 'custom' as const, x: drag.x, y: drag.y } } : cfg);
	const scene: PScene = $derived(w ? sceneAt(script.events, t, live, w, h) : { unit: 0, caps: [], pluses: [], clicks: [] });

	// A steady box for dragging: everything at its most visible.
	const box = $derived.by(() => {
		if (!w) return { x: 0, y: 0, w: 0, h: 0 };
		let best = bounds(sceneAt(script.events, 0, live, w, h));
		for (let s = 0.1; s < script.length; s += 0.1) {
			const b = bounds(sceneAt(script.events, s, live, w, h));
			if (b.w * b.h > best.w * best.h) best = b;
		}
		return best;
	});

	function down(e: PointerEvent) {
		if (!w || !box.w) return;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		drag = { x: box.x / w, y: box.y / h, px: e.clientX, py: e.clientY, ox: box.x / w, oy: box.y / h };
	}
	function move(e: PointerEvent) {
		if (!drag) return;
		const x = Math.min(Math.max(drag.ox + (e.clientX - drag.px) / w, 0), Math.max(0, 1 - box.w / w));
		const y = Math.min(Math.max(drag.oy + (e.clientY - drag.py) / h, 0), Math.max(0, 1 - box.h / h));
		drag = { ...drag, x, y };
	}
	function up() {
		if (!drag) return;
		const { x, y } = drag;
		drag = null;
		onmove(Math.round(x * 1000) / 1000, Math.round(y * 1000) / 1000);
	}
</script>

<div
	class="frame"
	class:dragging={!!drag}
	bind:clientWidth={w}
	style:height="{h}px"
	onpointerdown={down}
	onpointermove={move}
	onpointerup={up}
	onpointercancel={up}
	role="application"
	aria-label="Overlay preview. Drag to place the overlay."
>
	<div class="thirds" aria-hidden="true"></div>
	<OverlayScene {scene} {cfg} {h} />
</div>

<style>
	.frame {
		position: relative;
		width: 100%;
		overflow: hidden;
		border-radius: 6px;
		/* A stand-in game frame: dusk sky over a dark floor. Media colours, not theme. */
		background:
			linear-gradient(180deg, rgb(255 255 255 / 0) 58%, rgb(0 0 0 / 0.35) 58.2%),
			radial-gradient(120% 90% at 70% 20%, #3b4a63 0%, #1d2534 45%, #0d1118 100%);
		cursor: grab;
		touch-action: none;
	}
	.frame.dragging {
		cursor: grabbing;
	}
	.thirds {
		position: absolute;
		inset: 0;
		opacity: 0;
		transition: opacity 160ms var(--ease, ease);
		background:
			linear-gradient(90deg, transparent calc(33.33% - 0.5px), rgb(255 255 255 / 0.18) 0 calc(33.33% + 0.5px), transparent 0 calc(66.66% - 0.5px), rgb(255 255 255 / 0.18) 0 calc(66.66% + 0.5px), transparent 0),
			linear-gradient(180deg, transparent calc(33.33% - 0.5px), rgb(255 255 255 / 0.18) 0 calc(33.33% + 0.5px), transparent 0 calc(66.66% - 0.5px), rgb(255 255 255 / 0.18) 0 calc(66.66% + 0.5px), transparent 0);
	}
	.dragging .thirds {
		opacity: 1;
	}
</style>

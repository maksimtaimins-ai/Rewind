<!--
  The replay buffer as a live strip: "now" on the right edge, the oldest second
  a save would keep on the left. The ribbon is captured audio loudness, played
  out one 100 ms step at a time so it scrolls smoothly between status updates.
  Flags are markers; a save briefly lights up the stretch it kept.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { EngineState } from '#lib/api.ts';
	import { levelLog } from '#lib/stores/app.svelte.ts';

	let {
		capacity,
		held,
		levels,
		state,
		markers = [],
		flash = 0,
		flashSecs = capacity,
		onmarker
	}: {
		capacity: number;
		held: number;
		/** Newest loudness steps; each new array is appended to the history. */
		levels: number[];
		state: EngineState;
		markers?: { id: number; ago: number }[];
		/** Bump to light up the last `flashSecs` seconds. */
		flash?: number;
		flashSecs?: number;
		onmarker?: (id: number) => void;
	} = $props();

	const STEP_MS = 100;
	const live = $derived(state === 'recording');

	let canvas: HTMLCanvasElement;
	// Steps waiting to be played out, and the ones on screen (newest last).
	let pending: number[] = [];
	// Start from what was heard before this view opened (trimmed to the strip on the first frame).
	let history: number[] = levelLog.slice(-6000);
	let lastPop = 0;
	let flashAt = -Infinity;

	let seeded = false;
	$effect(() => {
		const fresh = levels;
		// The current steps are already in the log the history started from.
		if (!seeded) return void (seeded = true);
		pending.push(...fresh);
		// Never trail the real buffer by more than a second.
		if (pending.length > 10) pending = pending.slice(-10);
	});
	$effect(() => {
		if (flash) flashAt = performance.now();
	});

	const step = $derived(capacity <= 60 ? 10 : capacity <= 180 ? 30 : 60);
	const ticks = $derived(Array.from({ length: Math.floor(capacity / step) + 1 }, (_, i) => i * step).reverse());
	const label = (t: number) => (t === 0 ? '0s' : t < 60 || t % 60 ? `-${t}s` : `-${t / 60}m`);
	const visible = $derived(markers.filter((m) => m.ago <= capacity));

	/** Loudness on a log scale: quiet game audio still reads as a ribbon. */
	const height = (rms: number) => {
		if (rms <= 0) return 0;
		const db = 20 * Math.log10(rms);
		return Math.min(1, Math.max(0, (db + 54) / 48));
	};

	onMount(() => {
		const ctx = canvas.getContext('2d')!;
		const reduce = matchMedia('(prefers-reduced-motion: reduce)');
		let w = 0;
		let h = 0;
		let dpr = 1;
		let rgb = '231,231,229';
		let colorAt = 0;
		const ro = new ResizeObserver(() => {
			dpr = devicePixelRatio || 1;
			w = canvas.clientWidth;
			h = canvas.clientHeight;
			canvas.width = Math.round(w * dpr);
			canvas.height = Math.round(h * dpr);
		});
		ro.observe(canvas);

		/** The theme accent as "r,g,b", read back through the canvas so any CSS colour works. */
		const readAccent = () => {
			const css = getComputedStyle(canvas).getPropertyValue('--accent').trim();
			ctx.fillStyle = '#000';
			ctx.fillStyle = css || '#e7e7e5';
			const v = String(ctx.fillStyle);
			if (v.startsWith('#')) {
				const n = parseInt(v.slice(1), 16);
				rgb = `${(n >> 16) & 255},${(n >> 8) & 255},${n & 255}`;
			} else {
				const m = v.match(/[\d.]+/g);
				if (m) rgb = m.slice(0, 3).join(',');
			}
		};

		let raf = 0;
		const frame = (now: number) => {
			raf = requestAnimationFrame(frame);
			if (now - colorAt > 1000) {
				colorAt = now;
				readAccent();
			}
			const maxSteps = Math.ceil((capacity * 1000) / STEP_MS) + 2;
			while (now - lastPop >= STEP_MS) {
				// Out of fresh steps: let the last one fade rather than stall.
				const next = live ? (pending.shift() ?? (history.at(-1) ?? 0) * 0.85) : 0;
				history.push(next);
				lastPop = now - lastPop > STEP_MS * 4 ? now : lastPop + STEP_MS;
				if (history.length > maxSteps) history = history.slice(-maxSteps);
			}
			const frac = reduce.matches ? 0 : Math.min(1, (now - lastPop) / STEP_MS);
			draw(frac, now);
		};

		const draw = (frac: number, now: number) => {
			ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
			ctx.clearRect(0, 0, w, h);
			const mid = h / 2;
			const amp = h / 2 - 8;
			const pxPerSec = w / capacity;
			const heldX = w - Math.min(capacity, held) * pxPerSec;

			// Baseline: solid where the buffer holds footage, dotted where it doesn't yet.
			ctx.lineWidth = 1;
			ctx.strokeStyle = `rgba(${rgb},0.18)`;
			ctx.setLineDash([2, 4]);
			ctx.beginPath();
			ctx.moveTo(0, mid);
			ctx.lineTo(heldX, mid);
			ctx.stroke();
			ctx.setLineDash([]);
			ctx.beginPath();
			ctx.moveTo(heldX, mid);
			ctx.lineTo(w, mid);
			ctx.stroke();

			// Ribbon points, newest at the right edge, scrolled by the playout fraction.
			const pts: [number, number][] = [];
			for (let i = history.length - 1; i >= 0; i--) {
				const age = ((history.length - 1 - i + frac) * STEP_MS) / 1000;
				const x = w - age * pxPerSec;
				if (x < heldX - pxPerSec * 0.1) break;
				pts.push([x, Math.max(0.75, height(history[i]) * amp)]);
			}
			if (pts.length > 1) {
				const grad = ctx.createLinearGradient(0, 0, w, 0);
				grad.addColorStop(0, `rgba(${rgb},0.1)`);
				grad.addColorStop(0.7, `rgba(${rgb},0.45)`);
				grad.addColorStop(1, `rgba(${rgb},0.95)`);
				ctx.fillStyle = grad;
				ctx.beginPath();
				trace(pts, (y) => mid - y, true);
				trace([...pts].reverse(), (y) => mid + y, false);
				ctx.closePath();
				ctx.fill();
			}

			// A save lights up what it kept, then fades over 1.5 s.
			const t = (now - flashAt) / 1500;
			if (t >= 0 && t < 1) {
				const x0 = w - Math.min(capacity, flashSecs) * pxPerSec;
				const a = (1 - t) * (1 - t);
				ctx.fillStyle = `rgba(${rgb},${0.16 * a})`;
				ctx.fillRect(x0, 0, w - x0, h);
				ctx.fillStyle = `rgba(${rgb},${0.8 * a})`;
				ctx.fillRect(x0, 0, 1.5, h);
				// One soft band sweeps the saved stretch left to right.
				const sweep = Math.min(1, (now - flashAt) / 900);
				if (sweep < 1 && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
					const e = 1 - (1 - sweep) ** 3;
					const bw = Math.max(40, (w - x0) * 0.22);
					const cx = x0 - bw / 2 + (w - x0 + bw) * e;
					const g = ctx.createLinearGradient(cx - bw / 2, 0, cx + bw / 2, 0);
					const peak = 0.38 * Math.sin(Math.PI * sweep);
					g.addColorStop(0, `rgba(${rgb},0)`);
					g.addColorStop(0.5, `rgba(${rgb},${peak})`);
					g.addColorStop(1, `rgba(${rgb},0)`);
					ctx.fillStyle = g;
					ctx.fillRect(Math.max(x0, cx - bw / 2), 0, Math.min(bw, w - Math.max(x0, cx - bw / 2)), h);
				}
			}
		};

		/** Smooth curve through the points (midpoint quadratics). */
		const trace = (pts: [number, number][], y: (v: number) => number, start: boolean) => {
			if (start) ctx.moveTo(pts[0][0], y(pts[0][1]));
			else ctx.lineTo(pts[0][0], y(pts[0][1]));
			for (let i = 1; i < pts.length - 1; i++) {
				const mx = (pts[i][0] + pts[i + 1][0]) / 2;
				const my = (y(pts[i][1]) + y(pts[i + 1][1])) / 2;
				ctx.quadraticCurveTo(pts[i][0], y(pts[i][1]), mx, my);
			}
			const last = pts[pts.length - 1];
			ctx.lineTo(last[0], y(last[1]));
		};

		lastPop = performance.now();
		raf = requestAnimationFrame(frame);
		return () => {
			cancelAnimationFrame(raf);
			ro.disconnect();
		};
	});
</script>

<div class="timeline" class:live>
	<div class="strip" role="img" aria-label="Replay buffer: {Math.floor(held)} of {capacity} seconds held">
		<canvas bind:this={canvas}></canvas>
		{#each visible as m (m.id)}
			<button
				class="flag"
				style="right: {(m.ago / capacity) * 100}%"
				title="Marker set {Math.round(m.ago)}s ago. Click to save 30 seconds around it."
				aria-label="Save 30 seconds around the marker set {Math.round(m.ago)} seconds ago"
				onclick={() => onmarker?.(m.id)}
			>
				<span class="pennant"></span>
			</button>
		{/each}
		<span class="now" aria-hidden="true"></span>
		<span class="badge" aria-hidden="true">
			<span class="dot"></span>{state === 'paused' ? 'PAUSED' : live ? 'LIVE' : 'OFF'}
		</span>
	</div>
	<div class="scale num" aria-hidden="true">
		{#each ticks as t (t)}
			<span style="right: {(t / capacity) * 100}%" class:edge={t === capacity} class:zero={t === 0}>{label(t)}</span>
		{/each}
	</div>
</div>

<style>
	.timeline {
		display: grid;
		gap: 6px;
	}
	.strip {
		position: relative;
		height: 80px;
		border-radius: 8px;
		background: var(--sunken);
		box-shadow: inset 0 0 0 1px var(--line);
		overflow: hidden;
	}
	canvas {
		position: absolute;
		inset: 0;
		width: calc(100% - 2px);
		height: 100%;
	}
	.now {
		position: absolute;
		top: 0;
		bottom: 0;
		right: 1px;
		width: 2px;
		background: var(--line-strong);
	}
	.live .now {
		background: var(--signal);
	}
	.badge {
		position: absolute;
		top: 8px;
		right: 10px;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 7px 3px 6px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--sunken) 80%, transparent);
		color: var(--muted);
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 500;
		letter-spacing: 0.04em;
	}
	.live .badge {
		color: var(--text);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--faint);
	}
	.live .dot {
		background: var(--signal);
		animation: pulse 2s ease-in-out infinite;
	}
	@keyframes pulse {
		0%,
		100% {
			opacity: 0.6;
		}
		50% {
			opacity: 1;
		}
	}

	.flag {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 14px;
		margin-right: -7px;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
		/* Status arrives twice a second: glide between positions. */
		transition: right 500ms linear;
		animation: plant 220ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.flag::before {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: 6px;
		width: 2px;
		background: var(--accent-2);
		opacity: 0.85;
	}
	.pennant {
		position: absolute;
		top: 6px;
		left: 8px;
		width: 9px;
		height: 8px;
		background: var(--accent-2);
		clip-path: polygon(0 0, 100% 50%, 0 100%);
		transition: transform 120ms cubic-bezier(0.2, 0.8, 0.2, 1);
		transform-origin: left center;
	}
	.flag:hover .pennant,
	.flag:focus-visible .pennant {
		transform: scale(1.35);
	}
	@keyframes plant {
		from {
			opacity: 0;
			transform: translateY(-6px);
		}
	}

	.scale {
		position: relative;
		height: 14px;
		color: var(--faint);
		font-size: 10px;
	}
	.scale span {
		position: absolute;
		transform: translateX(50%);
	}
	/* End labels sit inside the strip's edges. */
	.scale .zero {
		transform: none;
	}
	.scale .edge {
		transform: none;
		right: auto !important;
		left: 0;
	}
</style>

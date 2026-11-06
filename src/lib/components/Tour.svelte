<!--
  First-run coachmarks: dims the window except one element at a time and
  points a short note at it. Targets are found by their data-tour attribute.
-->
<script lang="ts" module>
	export interface Step {
		target: string;
		title: string;
		body: string;
		keys?: string;
		/** Where the note sits relative to the target. */
		side: 'below' | 'right';
	}
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import Keycap from './Keycap.svelte';

	let { steps, ondone }: { steps: Step[]; ondone: () => void } = $props();

	let i = $state(0);
	let rect = $state<{ x: number; y: number; w: number; h: number } | null>(null);
	let next = $state<HTMLButtonElement>();
	const step = $derived(steps[i]);
	const last = $derived(i === steps.length - 1);
	const PAD = 6;
	const NOTE_W = 300;

	function measure() {
		const el = document.querySelector(`[data-tour="${step.target}"]`);
		if (!el) return void (rect = null);
		const r = el.getBoundingClientRect();
		rect = { x: r.left - PAD, y: r.top - PAD, w: r.width + PAD * 2, h: r.height + PAD * 2 };
	}

	$effect(() => {
		void step;
		const el = document.querySelector(`[data-tour="${step.target}"]`);
		el?.scrollIntoView({ block: 'nearest' });
		measure();
		next?.focus();
	});

	onMount(() => {
		const onresize = () => measure();
		addEventListener('resize', onresize);
		return () => removeEventListener('resize', onresize);
	});

	const advance = () => (last ? ondone() : i++);
	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') ondone();
		else if (e.key === 'ArrowRight') advance();
		else if (e.key === 'ArrowLeft' && i > 0) i--;
	}

	// Keep the note on screen: below (or above when there's no room), or to the right.
	const note = $derived.by(() => {
		if (!rect) return { left: innerWidth / 2 - NOTE_W / 2, top: innerHeight / 3, arrow: 'none' };
		if (step.side === 'right') {
			return { left: rect.x + rect.w + 14, top: Math.max(12, rect.y + rect.h / 2 - 28), arrow: 'left' };
		}
		const left = Math.min(innerWidth - NOTE_W - 16, Math.max(16, rect.x));
		const below = rect.y + rect.h + 14;
		return below + 170 < innerHeight ? { left, top: below, arrow: 'up' } : { left, top: rect.y - 14 - 160, arrow: 'down' };
	});
	// Arrow points at the target's middle, clamped inside the note.
	const arrowAt = $derived(rect ? Math.min(NOTE_W - 24, Math.max(16, rect.x + Math.min(rect.w, 280) / 2 - note.left)) : 0);
</script>

<svelte:window {onkeydown} />

<div class="tour" role="dialog" aria-modal="true" aria-labelledby="tour-title">
	<div class="shade" aria-hidden="true">
		{#if rect}
			<div class="hole" style="left: {rect.x}px; top: {rect.y}px; width: {rect.w}px; height: {rect.h}px"></div>
		{/if}
	</div>
	{#key i}
		<div class="note {note.arrow}" style="left: {note.left}px; top: {note.top}px; width: {NOTE_W}px; --arrow: {arrowAt}px">
			<p class="count num">{i + 1} of {steps.length}</p>
			<h2 id="tour-title">{step.title}</h2>
			<p class="body">
				{step.body}
				{#if step.keys}<span class="keys">Hotkey <Keycap keys={step.keys} /></span>{/if}
			</p>
			<div class="actions">
				{#if !last}<button class="skip" onclick={ondone}>Skip tour</button>{/if}
				<button class="next" bind:this={next} onclick={advance}>{last ? 'Done' : 'Next'}</button>
			</div>
		</div>
	{/key}
</div>

<style>
	.tour {
		position: fixed;
		inset: 0;
		z-index: 60;
	}
	.shade {
		position: absolute;
		inset: 0;
		animation: fade-in 180ms ease-out;
	}
	/* The hole's giant shadow is the dimming; the hole itself stays clear. */
	.hole {
		position: absolute;
		border-radius: 12px;
		box-shadow:
			0 0 0 2px var(--accent),
			0 0 0 9999px var(--scrim);
		transition:
			left 240ms var(--ease),
			top 240ms var(--ease),
			width 240ms var(--ease),
			height 240ms var(--ease);
	}
	.note {
		position: absolute;
		box-sizing: border-box;
		padding: 16px;
		border-radius: 12px;
		background: var(--raised);
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			0 16px 40px var(--scrim-soft);
		animation: modal-in 200ms ease-out;
	}
	.note::before {
		content: '';
		position: absolute;
		width: 10px;
		height: 10px;
		background: var(--raised);
		box-shadow: inset 1px 1px 0 var(--line-strong);
	}
	.note.up::before {
		top: -5px;
		left: var(--arrow);
		transform: rotate(45deg);
	}
	.note.down::before {
		bottom: -5px;
		left: var(--arrow);
		transform: rotate(225deg);
	}
	.note.left::before {
		left: -5px;
		top: 22px;
		transform: rotate(-45deg);
	}
	.note.none::before {
		display: none;
	}
	.count {
		margin: 0 0 6px;
		color: var(--faint);
		font-size: 11px;
	}
	h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	.body {
		margin: 6px 0 0;
		color: var(--muted);
		line-height: 1.5;
	}
	.keys {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 8px;
		color: var(--faint);
		font-size: 12px;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 12px;
		margin-top: 14px;
	}
	.skip {
		padding: 0;
		border: 0;
		background: none;
		color: var(--muted);
		font-size: 13px;
		cursor: pointer;
	}
	.skip:hover {
		color: var(--text);
	}
	.next {
		height: 32px;
		padding: 0 16px;
		border: 0;
		border-radius: 999px;
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
		cursor: pointer;
	}
</style>

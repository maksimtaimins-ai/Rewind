<!--
  The buffer as a strip of tape: "now" on the right, the oldest second the
  buffer can hold on the left. Ticks mark keyframes (where a clip may start),
  notches mark markers set with the marker hotkey.
-->
<script lang="ts">
	import { fmtDuration } from '#lib/api.ts';

	let {
		capacity,
		buffered,
		markers = [],
		recording = false,
		keyframeSecs = 1,
		onmarker
	}: {
		capacity: number;
		buffered: number;
		markers?: number[];
		recording?: boolean;
		keyframeSecs?: number;
		onmarker?: (index: number) => void;
	} = $props();

	const fill = $derived(Math.min(1, buffered / Math.max(1, capacity)));
	// Keep the tick density readable for long buffers.
	const step = $derived(capacity <= 90 ? keyframeSecs : capacity <= 300 ? 5 : 15);
	const ticks = $derived(Array.from({ length: Math.floor(capacity / step) + 1 }, (_, i) => i * step));
	const major = $derived(capacity <= 90 ? 10 : capacity <= 300 ? 30 : 60);
</script>

<div class="tape" role="img" aria-label="Replay buffer: {Math.round(buffered)} of {capacity} seconds held">
	<div class="track">
		<div class="fill" class:live={recording} style="width: {fill * 100}%"></div>
		{#each ticks as t (t)}
			<span class="tick" class:major={t % major === 0} style="right: {(t / capacity) * 100}%"></span>
		{/each}
		{#each markers as ago, i (i)}
			{#if ago <= capacity}
				<button
					class="marker"
					style="right: {(ago / capacity) * 100}%"
					title="Marker {fmtDuration(ago)} ago. Click to save around it."
					onclick={() => onmarker?.(i)}
					aria-label="Save around marker set {Math.round(ago)} seconds ago"
				></button>
			{/if}
		{/each}
	</div>
	<div class="scale num">
		{#each ticks.filter((t) => t % major === 0) as t (t)}
			<span class:edge={t >= capacity} style="right: {(t / capacity) * 100}%">{t === 0 ? 'now' : `−${fmtDuration(t)}`}</span>
		{/each}
	</div>
</div>

<style>
	.tape {
		width: 100%;
	}
	.track {
		position: relative;
		height: 72px;
		background: var(--sunken);
		border-radius: 4px;
		overflow: hidden;
	}
	.fill {
		position: absolute;
		top: 0;
		bottom: 0;
		right: 0;
		background: var(--line);
		transition: width 500ms linear;
	}
	/* The write head: the one place red appears on the tape. */
	.fill.live::after {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		right: 0;
		width: 3px;
		background: var(--signal);
	}
	.tick {
		position: absolute;
		bottom: 0;
		width: 1px;
		height: 8px;
		background: var(--line-strong);
		transform: translateX(50%);
	}
	.tick.major {
		height: 16px;
		background: var(--muted);
	}
	.marker {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 12px;
		margin-right: -6px;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: pointer;
	}
	.marker::before {
		content: '';
		position: absolute;
		left: 50%;
		top: 0;
		bottom: 0;
		width: 2px;
		margin-left: -1px;
		background: var(--accent-2);
	}
	.marker::after {
		content: '';
		position: absolute;
		left: 50%;
		top: 0;
		margin-left: -6px;
		border: 6px solid transparent;
		border-top-color: var(--accent-2);
	}
	.scale {
		position: relative;
		height: 24px;
		color: var(--faint);
		font-size: 11px;
	}
	.scale span {
		position: absolute;
		top: 8px;
		transform: translateX(50%);
		white-space: nowrap;
	}
	.scale span:first-child {
		transform: none;
	}
	.scale span.edge {
		transform: translateX(100%);
	}
</style>

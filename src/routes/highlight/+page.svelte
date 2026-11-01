<!-- Recording highlight: drawn in a transparent, click-through window over the
     primary display. The backend shows and hides the window; this only draws. -->
<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';
	import { highlightColor } from '#lib/highlight.ts';

	const h = $derived(app.settings!.highlight);
	const color = $derived(highlightColor(h));
	// Corner dots grow with thickness: 2px gives a 12px dot, 10px a 28px one.
	const dot = $derived(8 + h.thickness * 2);
</script>

<svelte:head><title>Rewind recording highlight</title></svelte:head>

{#if color && h.thickness > 0}
	{#if h.position === 'border'}
		<div class="border" style:--c={color} style:--w="{h.thickness}px" style:opacity={h.opacity / 100}></div>
	{:else}
		<div class="dot {h.position}" style:--c={color} style:--d="{dot}px" style:opacity={h.opacity / 100}></div>
	{/if}
{/if}

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: hidden;
		pointer-events: none;
	}
	.border {
		position: fixed;
		inset: 0;
		border: var(--w) solid var(--c);
		box-shadow:
			inset 0 0 calc(var(--w) * 4) var(--c),
			inset 0 0 calc(var(--w) * 1.5) var(--c);
	}
	.dot {
		position: fixed;
		width: var(--d);
		height: var(--d);
		border-radius: 50%;
		background: var(--c);
		box-shadow: 0 0 calc(var(--d) / 2) var(--c);
	}
	.top_left {
		top: 16px;
		left: 16px;
	}
	.top_right {
		top: 16px;
		right: 16px;
	}
	.bottom_right {
		bottom: 16px;
		right: 16px;
	}
</style>

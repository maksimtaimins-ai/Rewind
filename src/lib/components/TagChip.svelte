<!-- A tag in the colour picked for it. Clickable to filter; optionally removable. -->
<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';

	let {
		tag,
		active = false,
		onclick,
		onremove
	}: { tag: string; active?: boolean; onclick?: () => void; onremove?: () => void } = $props();

	const color = $derived(app.meta.tag_colors[tag] ?? 'var(--muted)');
</script>

<span class="chip" class:active style="--tag: {color}">
	<button class="label" onclick={(e) => (e.stopPropagation(), onclick?.())} disabled={!onclick} aria-pressed={onclick ? active : undefined}>
		<span class="dot" aria-hidden="true"></span>{tag}
	</button>
	{#if onremove}
		<button class="x" aria-label="Remove tag {tag}" onclick={(e) => (e.stopPropagation(), onremove())}>×</button>
	{/if}
</span>

<style>
	.chip {
		display: inline-flex;
		align-items: center;
		height: 24px;
		border-radius: 4px;
		background: color-mix(in srgb, var(--tag) 16%, transparent);
		box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tag) 30%, transparent);
		font-size: 12px;
		max-width: 100%;
	}
	.chip.active {
		background: color-mix(in srgb, var(--tag) 34%, transparent);
		box-shadow: inset 0 0 0 1px var(--tag);
	}
	button {
		border: 0;
		background: none;
		color: var(--text);
		cursor: pointer;
	}
	button:disabled {
		cursor: default;
	}
	.label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 0 8px;
		height: 100%;
		min-width: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dot {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--tag);
	}
	.x {
		padding: 0 6px 0 0;
		color: var(--muted);
		font-size: 14px;
	}
	.x:hover {
		color: var(--text);
	}
</style>

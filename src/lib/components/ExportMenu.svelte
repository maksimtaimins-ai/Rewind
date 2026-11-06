<!-- "Export" button with destination presets. Each preset re-encodes a copy next to the clip. -->
<script lang="ts">
	import type { ExportPreset } from '#lib/api.ts';

	import Toggle from './Toggle.svelte';

	// `keys`: the clip has recorded keystrokes, so offer to draw them in.
	let { busy = false, keys = false, onexport }: { busy?: boolean; keys?: boolean; onexport: (p: ExportPreset, overlay: boolean) => void } = $props();
	let overlay = $state(true);

	const presets: { id: ExportPreset; name: string; what: string }[] = [
		{ id: 'discord10', name: 'Discord, 10 MB', what: 'Free upload limit. Long clips are cut to fit.' },
		{ id: 'discord25', name: 'Discord, 25 MB', what: 'Nitro Basic limit, sharper.' },
		{ id: 'shorts', name: 'YouTube Shorts', what: 'Vertical 1080×1920 centre crop, up to 60s.' },
		{ id: 'twitter', name: 'X / Twitter', what: 'Up to 2:20, 1080p.' },
		{ id: 'gif', name: 'GIF', what: 'Up to 15s, 480p, no sound.' }
	];

	let open = $state(false);
	let root = $state<HTMLDivElement>();

	function pick(p: ExportPreset) {
		open = false;
		onexport(p, keys && overlay);
	}
</script>

<svelte:window
	onpointerdown={(e) => open && !root?.contains(e.target as Node) && (open = false)}
	onkeydown={(e) => open && e.key === 'Escape' && (open = false)}
/>

<div class="wrap" bind:this={root}>
	<button class="secondary" disabled={busy} aria-haspopup="menu" aria-expanded={open} onclick={() => (open = !open)}>
		{busy ? 'Exporting…' : 'Export'}
	</button>
	{#if open}
		<div class="menu" role="menu">
			{#if keys}
				<div class="opt">
					<span><span class="name">Draw keystrokes</span><span class="what">Bakes the recorded keys into the copy, styled in Settings.</span></span>
					<Toggle checked={overlay} label="Draw keystrokes" onchange={(v) => (overlay = v)} />
				</div>
			{/if}
			{#each presets as p (p.id)}
				<button role="menuitem" onclick={() => pick(p.id)}>
					<span class="name">{p.name}</span>
					<span class="what">{p.what}</span>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.wrap {
		position: relative;
	}
	.secondary {
		height: 36px;
		padding: 0 16px;
		border: 0;
		border-radius: 4px;
		background: var(--raised);
		font-weight: 600;
		cursor: pointer;
	}
	.secondary:hover:not(:disabled) {
		background: var(--line);
	}
	.secondary:disabled {
		color: var(--faint);
		cursor: default;
	}
	.menu {
		position: absolute;
		right: 0;
		bottom: calc(100% + 8px);
		z-index: 20;
		width: 280px;
		padding: 4px;
		display: grid;
		border-radius: 8px;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--line-strong);
	}
	.menu button {
		display: grid;
		gap: 2px;
		padding: 8px 12px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		text-align: left;
		cursor: pointer;
	}
	.menu button:hover,
	.menu button:focus-visible {
		background: var(--line);
	}
	.name {
		font-weight: 500;
	}
	.opt {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 12px 10px;
		margin-bottom: 4px;
		box-shadow: 0 1px 0 var(--line-strong);
	}
	.opt > span {
		display: grid;
		gap: 2px;
	}
	.what {
		color: var(--muted);
		font-size: 12px;
	}
</style>

<!--
  Settings → Library → View: how the clip library looks. All of it is read by
  the library page and its tiles; none of it touches capture.
-->
<script lang="ts">
	import type { LibrarySort, Settings } from '#lib/api.ts';
	import { app, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from './Row.svelte';
	import Segmented from './Segmented.svelte';

	type Lib = Settings['library'];
	const lib = $derived(app.settings!.library);
	const set = (f: (l: Lib) => void) => updateSettings((x) => f(x.library));

	// Presets are marks on the slider, not a separate setting.
	const sizes = [
		{ value: 160, label: 'Small' },
		{ value: 240, label: 'Medium' },
		{ value: 340, label: 'Large' },
		{ value: 400, label: 'XL' }
	];
	const preset = $derived(sizes.find((s) => s.value === lib.tile_size)?.value ?? -1);

	const sorts: { value: LibrarySort; label: string }[] = [
		{ value: 'newest', label: 'Newest first' },
		{ value: 'oldest', label: 'Oldest first' },
		{ value: 'name', label: 'Name' },
		{ value: 'duration', label: 'Longest first' },
		{ value: 'game', label: 'Game' },
		{ value: 'size', label: 'Largest first' }
	];
	const facts = [
		['duration', 'Duration'],
		['date', 'Date'],
		['game', 'Game'],
		['tags', 'Tags'],
		['size', 'Size']
	] as const;
</script>

<h3 id="library-view">View</h3>
<Row id="s-library-layout" label="Layout" hint="Tiles, or one clip per row.">
	<Segmented
		label="Library layout"
		value={lib.layout}
		options={[
			{ value: 'grid' as const, label: 'Grid' },
			{ value: 'list' as const, label: 'List' }
		]}
		onchange={(v) => set((l) => (l.layout = v))}
	/>
</Row>
{#if lib.layout === 'grid'}
	<Row id="s-tile-size" label="Tile size" hint="Smallest a tile may get; they stretch to fill the row.">
		<Segmented label="Tile size preset" value={preset} options={sizes} onchange={(v) => set((l) => (l.tile_size = v))} />
		<input type="range" min="120" max="400" step="10" value={lib.tile_size} oninput={(e) => set((l) => (l.tile_size = Number(e.currentTarget.value)))} aria-label="Tile size in pixels" />
		<span class="readout num">{lib.tile_size}px</span>
	</Row>
{/if}
<Row id="s-density" label="Density" hint="Space between tiles.">
	<Segmented
		label="Library density"
		value={lib.density}
		options={[
			{ value: 'compact' as const, label: 'Compact' },
			{ value: 'comfortable' as const, label: 'Comfortable' },
			{ value: 'spacious' as const, label: 'Spacious' }
		]}
		onchange={(v) => set((l) => (l.density = v))}
	/>
</Row>
<Row id="s-thumb-style" label="Thumbnails" hint="Original shows the whole frame without cropping.">
	<Segmented
		label="Thumbnail style"
		value={lib.thumb_style}
		options={[
			{ value: 'square' as const, label: 'Square' },
			{ value: 'wide' as const, label: '16:9' },
			{ value: 'original' as const, label: 'Original' }
		]}
		onchange={(v) => set((l) => (l.thumb_style = v))}
	/>
</Row>
<Row id="s-library-sort" label="Sort by" hint={lib.sort === 'newest' || lib.sort === 'oldest' ? 'Grouped into sessions.' : 'One list, no session groups.'}>
	<select value={lib.sort} onchange={(e) => set((l) => (l.sort = e.currentTarget.value as LibrarySort))} aria-label="Default sort">
		{#each sorts as o (o.value)}
			<option value={o.value}>{o.label}</option>
		{/each}
	</select>
</Row>
<Row id="s-tile-facts" label="Show on tiles" hint="The name is always shown.">
	<div class="facts" role="group" aria-label="Details on tiles">
		{#each facts as [key, label] (key)}
			<button class="fact" class:on={lib.meta[key]} aria-pressed={lib.meta[key]} onclick={() => set((l) => (l.meta[key] = !l.meta[key]))}>{label}</button>
		{/each}
	</div>
</Row>

<style>
	select {
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
	}
	input[type='range'] {
		width: 120px;
		accent-color: var(--text);
	}
	.readout {
		width: 52px;
		text-align: right;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		justify-content: flex-end;
	}
	.fact {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 14px;
		background: var(--sunken);
		color: var(--muted);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}
	.fact:hover {
		color: var(--text);
	}
	.fact.on {
		background: var(--accent);
		color: var(--on-accent);
	}
</style>

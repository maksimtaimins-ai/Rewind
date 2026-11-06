<!--
  Rewind's disk use as one thin segmented bar, coloured from the theme.
  Full: label and total above, legend below. Mini: just the bar, for the
  dashboard's status line. Hover a segment for exact bytes and file count.
-->
<script lang="ts">
	import { fmtBytes, type StorageBreakdown, type StorageKind } from '#lib/api.ts';

	let { data, mini = false }: { data: StorageBreakdown | null; mini?: boolean } = $props();

	const names: Record<StorageKind, string> = {
		clips: 'Clips',
		screenshots: 'Screenshots',
		exports: 'Exports',
		recycled: 'Recently deleted',
		thumbnails: 'Thumbnails',
		app_data: 'App data'
	};
	// Steps of the theme's accent, then its quiet greys: no colours of our own.
	const tones: Record<StorageKind, string> = {
		clips: 'var(--accent)',
		exports: 'color-mix(in srgb, var(--accent) 62%, var(--ground))',
		screenshots: 'color-mix(in srgb, var(--accent) 38%, var(--ground))',
		recycled: 'var(--muted)',
		thumbnails: 'var(--faint)',
		app_data: 'var(--line-strong)'
	};
	const order: StorageKind[] = ['clips', 'exports', 'screenshots', 'recycled', 'thumbnails', 'app_data'];

	const cats = $derived(data ? order.map((k) => data.categories.find((c) => c.kind === k)!).filter(Boolean) : []);
	const shown = $derived(cats.filter((c) => c.bytes > 0));
	const total = $derived(data?.total ?? 0);
	let hover = $state<StorageKind | null>(null);
	const hovered = $derived(hover ? cats.find((c) => c.kind === hover) : null);
	let tipX = $state(0);

	const gb = (b: number) => (b / 1e9 >= 100 ? (b / 1e9).toFixed(0) : (b / 1e9).toFixed(1));
	const plural = (n: number, w: string) => `${n.toLocaleString()} ${w}${n === 1 ? '' : 's'}`;

	function enter(kind: StorageKind, e: PointerEvent) {
		hover = kind;
		const el = e.currentTarget as HTMLElement;
		tipX = el.offsetLeft + el.offsetWidth / 2;
	}
</script>

<div class="storage" class:mini>
	{#if !mini}
		<div class="top">
			<span class="what">Rewind's files</span>
			<span class="used num">
				{#if data}
					{gb(total)} GB{#if data.drive_total}{' '}of {gb(data.drive_total)} GB used{/if}
				{:else}
					Measuring…
				{/if}
			</span>
		</div>
	{/if}
	<div class="track" role="group" aria-label={data ? `Rewind uses ${fmtBytes(total)}: ${shown.map((c) => `${names[c.kind]} ${fmtBytes(c.bytes)}`).join(', ')}` : 'Measuring storage'} onpointerleave={() => (hover = null)}>
		{#each shown as c (c.kind)}
			<span
				class="seg"
				class:dim={hover && hover !== c.kind}
				style:flex-grow={c.bytes}
				style:background={tones[c.kind]}
				role="img"
				aria-label="{names[c.kind]}: {fmtBytes(c.bytes)}, {plural(c.files, 'file')}"
				onpointerenter={(e) => enter(c.kind, e)}
			></span>
		{/each}
		{#if hovered}
			<span class="tip" style:left="{tipX}px" role="tooltip">
				<span class="tip-name">{names[hovered.kind]}</span>
				<span class="num">{hovered.bytes.toLocaleString()} bytes</span>
				<span class="tip-files">{plural(hovered.files, 'file')}</span>
			</span>
		{/if}
	</div>
	{#if !mini}
		<ul class="legend">
			{#each cats as c (c.kind)}
				<li class:dim={hover && hover !== c.kind} onpointerenter={() => (hover = c.kind)} onpointerleave={() => (hover = null)}>
					<span class="dot" style:background={tones[c.kind]}></span>
					<span class="name">{names[c.kind]}</span>
					<span class="size num">{c.bytes ? fmtBytes(c.bytes) : '0'}</span>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.storage {
		width: min(420px, 100%);
	}
	.storage.mini {
		width: 120px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		margin-bottom: 8px;
	}
	.what {
		color: var(--muted);
		font-size: 13px;
	}
	.used {
		font-size: 12px;
		color: var(--text);
	}
	.track {
		position: relative;
		display: flex;
		gap: 1px;
		height: 10px;
		border-radius: 4px;
		/* The gaps between segments show the page through as 1px dividers. */
		background: var(--ground);
		box-shadow: 0 0 0 1px var(--line);
	}
	.track:empty,
	.track:not(:has(.seg)) {
		background: var(--sunken);
	}
	.mini .track {
		height: 4px;
		border-radius: 2px;
		box-shadow: none;
	}
	.seg {
		flex-basis: 0;
		min-width: 3px;
		transition: opacity 140ms var(--ease);
	}
	.seg:first-of-type {
		border-radius: 4px 0 0 5px;
	}
	.seg:last-of-type {
		border-radius: 0 5px 5px 0;
	}
	.seg:only-of-type {
		border-radius: 4px;
	}
	.mini .seg {
		min-width: 2px;
	}
	.mini .seg:first-of-type {
		border-radius: 2px 0 0 2px;
	}
	.mini .seg:last-of-type {
		border-radius: 0 2px 2px 0;
	}
	.dim {
		opacity: 0.35;
	}
	.tip {
		position: absolute;
		bottom: calc(100% + 8px);
		transform: translateX(-50%);
		z-index: 30;
		display: grid;
		gap: 1px;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--raised);
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			0 6px 18px var(--scrim-soft);
		color: var(--text);
		font-size: 12px;
		white-space: nowrap;
		pointer-events: none;
	}
	.tip-name {
		font-weight: 600;
	}
	.tip-files {
		color: var(--muted);
	}
	.legend {
		list-style: none;
		margin: 12px 0 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 6px 18px;
	}
	.legend li {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		font-size: 12px;
		transition: opacity 140ms var(--ease);
	}
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}
	.name {
		color: var(--muted);
		white-space: nowrap;
	}
	.size {
		margin-left: 2px;
		color: var(--text);
		font-size: 11px;
		white-space: nowrap;
	}
</style>

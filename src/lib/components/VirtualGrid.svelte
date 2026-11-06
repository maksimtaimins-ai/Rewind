<!--
  A grid that renders only the rows on screen once it holds `threshold` items or more.
  Below that it is a plain CSS grid. Tiles must have one fixed height so rows can be placed
  without measuring every one: the first rendered row sets the height.
-->
<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';

	let {
		items,
		key,
		item,
		minWidth = 320,
		gap = 24,
		threshold = 100,
		overscan = 2
	}: {
		items: T[];
		key: (t: T) => string;
		item: Snippet<[T]>;
		minWidth?: number;
		gap?: number;
		threshold?: number;
		/** Extra rows rendered above and below the visible ones. */
		overscan?: number;
	} = $props();

	let el = $state<HTMLDivElement>();
	let width = $state(0);
	let rowH = $state(0);
	let first = $state(0);
	let last = $state(0);

	const virtual = $derived(items.length >= threshold);
	const cols = $derived(Math.max(1, Math.floor((width + gap) / (minWidth + gap))));
	const rows = $derived(Math.ceil(items.length / cols));
	// Before a row is measured, guess from a 16:9 tile plus a line of text.
	const step = $derived((rowH || ((width - gap * (cols - 1)) / cols) * (9 / 16) + 56) + gap);
	const slice = $derived(virtual ? items.slice(first * cols, last * cols) : items);

	function scroller(node: HTMLElement): HTMLElement | null {
		for (let p = node.parentElement; p; p = p.parentElement) {
			const o = getComputedStyle(p).overflowY;
			if (o === 'auto' || o === 'scroll') return p;
		}
		return null;
	}

	function measure() {
		if (!el || !virtual) return;
		const root = scroller(el);
		const view = root ? root.clientHeight : innerHeight;
		const top = el.getBoundingClientRect().top - (root ? root.getBoundingClientRect().top : 0);
		const from = Math.floor(-top / step) - overscan;
		const to = Math.ceil((view - top) / step) + overscan;
		first = Math.max(0, Math.min(rows, from));
		last = Math.max(first, Math.min(rows, to));
		const row = el.querySelector<HTMLElement>('.vcell > *');
		if (row && row.offsetHeight > 0 && Math.abs(row.offsetHeight - rowH) > 1) rowH = row.offsetHeight;
	}

	$effect(() => {
		if (!el) return;
		const root = scroller(el) ?? window;
		let raf = 0;
		const queue = () => {
			cancelAnimationFrame(raf);
			raf = requestAnimationFrame(measure);
		};
		const ro = new ResizeObserver(() => {
			width = el!.clientWidth;
			queue();
		});
		ro.observe(el);
		if (root !== window) ro.observe(root as HTMLElement);
		root.addEventListener('scroll', queue, { passive: true });
		queue();
		return () => {
			cancelAnimationFrame(raf);
			ro.disconnect();
			root.removeEventListener('scroll', queue);
		};
	});

	// Re-measure when the list or the layout changes.
	$effect(() => {
		void [items.length, cols, step, virtual];
		measure();
	});
</script>

<div
	bind:this={el}
	class="vgrid"
	style:--min="{minWidth}px"
	style:--gap="{gap}px"
	style:height={virtual ? `${Math.max(0, rows * step - gap)}px` : undefined}
>
	<div class="vrows" style:grid-template-columns="repeat({cols}, minmax(0, 1fr))" style:transform={virtual ? `translateY(${first * step}px)` : undefined}>
		{#each slice as it (key(it))}
			<div class="vcell">{@render item(it)}</div>
		{/each}
	</div>
</div>

<style>
	.vgrid {
		position: relative;
		width: 100%;
	}
	.vrows {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--min), 1fr));
		gap: var(--gap);
	}
	.vcell {
		min-width: 0;
	}
</style>

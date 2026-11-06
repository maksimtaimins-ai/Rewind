<!--
  A vertical list the user can reorder by dragging a row, or with the arrow
  buttons (for keyboard and touch). `onchange` gets the new id order once a
  drag ends or an arrow is used. The rows themselves come from `row`.
-->
<script lang="ts" generics="T extends { id: string }">
	import type { Snippet } from 'svelte';

	let {
		items,
		row,
		onchange,
		label
	}: {
		items: T[];
		row: Snippet<[T, number]>;
		onchange: (ids: string[]) => void;
		label: string;
	} = $props();

	// While dragging, rows move on screen only; the drop saves.
	let order = $state<string[] | null>(null);
	let dragging = $state<string | null>(null);

	const ids = $derived(order ?? items.map((i) => i.id));
	const shown = $derived(ids.map((id) => items.find((i) => i.id === id)).filter((i): i is T => !!i));

	function over(target: string) {
		if (!dragging || dragging === target) return;
		const next = ids.filter((i) => i !== dragging);
		next.splice(next.indexOf(target) + (ids.indexOf(dragging) < ids.indexOf(target) ? 1 : 0), 0, dragging);
		order = next;
	}

	function drop() {
		const done = order;
		order = null;
		dragging = null;
		if (done && done.join() !== items.map((i) => i.id).join()) onchange(done);
	}

	function nudge(i: number, by: number) {
		const j = i + by;
		if (j < 0 || j >= ids.length) return;
		const next = [...ids];
		[next[i], next[j]] = [next[j], next[i]];
		onchange(next);
	}
</script>

<ul class="sortable" aria-label={label}>
	{#each shown as it, i (it.id)}
		<li
			class:dragging={dragging === it.id}
			draggable="true"
			ondragstart={(e) => {
				dragging = it.id;
				e.dataTransfer?.setData('text/plain', it.id);
				if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
			}}
			ondragover={(e) => {
				e.preventDefault();
				over(it.id);
			}}
			ondrop={(e) => {
				e.preventDefault();
				drop();
			}}
			ondragend={drop}
		>
			<span class="grip" aria-hidden="true">
				<svg viewBox="0 0 10 16"><circle cx="3" cy="3" r="1.2" /><circle cx="7" cy="3" r="1.2" /><circle cx="3" cy="8" r="1.2" /><circle cx="7" cy="8" r="1.2" /><circle cx="3" cy="13" r="1.2" /><circle cx="7" cy="13" r="1.2" /></svg>
			</span>
			<div class="body">{@render row(it, i)}</div>
			<span class="nudge">
				<button aria-label="Move up" disabled={i === 0} onclick={() => nudge(i, -1)}>↑</button>
				<button aria-label="Move down" disabled={i === shown.length - 1} onclick={() => nudge(i, 1)}>↓</button>
			</span>
		</li>
	{/each}
</ul>

<style>
	.sortable {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		border-radius: var(--r-1);
		background: var(--sunken);
		cursor: grab;
	}
	li.dragging {
		opacity: 0.55;
	}
	.grip {
		width: 10px;
		flex: none;
		color: var(--faint);
	}
	.grip svg {
		width: 10px;
		height: 16px;
		fill: currentColor;
	}
	.body {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.nudge {
		display: flex;
		gap: 2px;
	}
	.nudge button {
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.nudge button:hover:not(:disabled) {
		color: var(--text);
		background: var(--line);
	}
	.nudge button:disabled {
		opacity: 0.3;
		cursor: default;
	}
</style>

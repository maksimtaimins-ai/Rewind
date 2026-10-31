<script lang="ts" generics="T extends string | number">
	let {
		value,
		options,
		label,
		onchange
	}: {
		value: T;
		options: { value: T; label: string }[];
		label: string;
		onchange: (v: T) => void;
	} = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label}>
	{#each options as o (o.value)}
		<button role="radio" aria-checked={o.value === value} class:on={o.value === value} onclick={() => onchange(o.value)}>
			{o.label}
		</button>
	{/each}
</div>

<style>
	.seg {
		display: inline-flex;
		padding: 2px;
		gap: 2px;
		background: var(--sunken);
		border-radius: 4px;
	}
	button {
		padding: 4px 12px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
		white-space: nowrap;
	}
	button:hover {
		color: var(--text);
	}
	button.on {
		background: var(--line-strong);
		color: var(--text);
	}
</style>

<!-- A hotkey drawn as keycaps: "Alt+F8" becomes two small keys. Renders nothing for an unset hotkey. -->
<script lang="ts">
	let { keys, tone = 'plain' }: { keys: string | null | undefined; tone?: 'plain' | 'on-accent' } = $props();
	const parts = $derived(keys ? keys.split('+').filter(Boolean) : []);
</script>

{#if parts.length}
	<kbd class="keys {tone}" aria-label="Hotkey {keys}">
		{#each parts as k, i (i)}<span class="cap">{k}</span>{/each}
	</kbd>
{/if}

<style>
	.keys {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-family: var(--font-mono);
		font-size: 10.5px;
		font-weight: 500;
		line-height: 1;
		letter-spacing: -0.02em;
		white-space: nowrap;
	}
	.cap {
		padding: 3px 5px 2px;
		border-radius: 4px;
		color: var(--muted);
		background: var(--ground);
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			inset 0 -1px 0 1px var(--line-strong);
	}
	.on-accent .cap {
		color: var(--on-accent);
		background: color-mix(in srgb, var(--on-accent) 12%, transparent);
		box-shadow:
			inset 0 0 0 1px color-mix(in srgb, var(--on-accent) 30%, transparent),
			inset 0 -1px 0 1px color-mix(in srgb, var(--on-accent) 30%, transparent);
	}
</style>

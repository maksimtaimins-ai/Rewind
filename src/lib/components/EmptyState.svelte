<!--
  What an empty list says: one line of what goes here and how to get it there.
  `keys` draws the hotkey that fills it as a large keycap being pressed.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let { keys, compact = false, children }: { keys?: string | null; compact?: boolean; children: Snippet } = $props();
	const parts = $derived(keys ? keys.split('+').filter(Boolean) : []);
</script>

<div class="empty" class:compact>
	{#if parts.length && !compact}
		<div class="press" aria-hidden="true">
			{#each parts as k, i (i)}
				{#if i}<span class="plus">+</span>{/if}
				<span class="cap" style="animation-delay: {i * 60}ms">{k}</span>
			{/each}
		</div>
	{:else if !compact}
		<div class="beacon" aria-hidden="true">
			<span class="ring r1"></span>
			<span class="ring r2"></span>
			<span class="dot"></span>
		</div>
	{/if}
	<p>{@render children()}</p>
</div>

<style>
	.empty {
		display: grid;
		justify-items: start;
		gap: 16px;
		padding: 32px 0;
		color: var(--muted);
	}
	.empty.compact {
		padding: 0;
		gap: 0;
		font-size: 13px;
	}
	.compact p {
		max-width: 80ch;
	}
	p {
		margin: 0;
		max-width: 52ch;
		line-height: 1.6;
	}
	.press {
		display: flex;
		align-items: center;
		gap: 8px;
		font-family: var(--font-mono);
		font-weight: 500;
	}
	.plus {
		color: var(--faint);
	}
	/* No key to show: a buffer dot that keeps recording, with rings spreading from it. */
	.beacon {
		position: relative;
		width: 44px;
		height: 44px;
		display: grid;
		place-items: center;
	}
	.dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--faint);
	}
	.ring {
		position: absolute;
		inset: 8px;
		border-radius: 50%;
		box-shadow: inset 0 0 0 1.5px var(--line-strong);
		animation: spread 2.8s var(--ease) infinite;
	}
	.r2 {
		animation-delay: 1.4s;
	}
	@keyframes spread {
		from {
			transform: scale(0.4);
			opacity: 1;
		}
		to {
			transform: scale(1.6);
			opacity: 0;
		}
	}
	/* A key that gets pressed every couple of seconds. */
	.cap {
		min-width: 44px;
		height: 44px;
		padding: 0 12px;
		box-sizing: border-box;
		display: grid;
		place-items: center;
		border-radius: 8px;
		background: var(--raised);
		color: var(--text);
		font-size: 15px;
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			0 4px 0 0 var(--line-strong);
		animation: tap 2.4s var(--ease) infinite;
	}
	@keyframes tap {
		0%,
		70%,
		100% {
			transform: translateY(0);
			box-shadow:
				inset 0 0 0 1px var(--line-strong),
				0 4px 0 0 var(--line-strong);
		}
		78% {
			transform: translateY(3px);
			box-shadow:
				inset 0 0 0 1px var(--accent),
				0 1px 0 0 var(--line-strong);
		}
	}
</style>

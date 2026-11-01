<!--
  One setting: label with a one-line explanation below it on the left, control on the right.
  `help` adds a "?" that opens a plain-English explanation for technical settings.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	let { label, hint, help, children }: { label: string; hint?: string; help?: string; children: Snippet } = $props();

	let open = $state(false);
	let wrap = $state<HTMLElement>();
	$effect(() => {
		if (!open) return;
		const close = (e: Event) => {
			if (e instanceof KeyboardEvent ? e.key === 'Escape' : !wrap?.contains(e.target as Node)) open = false;
		};
		window.addEventListener('pointerdown', close);
		window.addEventListener('keydown', close);
		return () => {
			window.removeEventListener('pointerdown', close);
			window.removeEventListener('keydown', close);
		};
	});
</script>

<div class="row">
	<div class="copy">
		<div class="label">
			{label}
			{#if help}
				<span class="help" bind:this={wrap}>
					<button class="q" aria-expanded={open} aria-label="What is {label}?" onclick={() => (open = !open)}>?</button>
					{#if open}<span class="pop" role="note">{help}</span>{/if}
				</span>
			{/if}
		</div>
		{#if hint}<div class="hint">{hint}</div>{/if}
	</div>
	<div class="control">{@render children()}</div>
</div>

<style>
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		padding: 12px 0;
	}
	.copy {
		min-width: 0;
		max-width: 60ch;
	}
	.label {
		font-weight: 500;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.hint {
		color: var(--muted);
		font-size: 13px;
		margin-top: 4px;
	}
	.control {
		flex: none;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.help {
		position: relative;
		display: inline-flex;
	}
	.q {
		width: 16px;
		height: 16px;
		padding: 0;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 50%;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--line-strong);
		color: var(--muted);
		font-size: 10.5px;
		font-weight: 650;
		line-height: 1;
		cursor: pointer;
	}
	.q:hover,
	.q[aria-expanded='true'] {
		color: var(--text);
	}
	.pop {
		position: absolute;
		left: -8px;
		top: calc(100% + 8px);
		z-index: 30;
		width: 320px;
		padding: 12px 14px;
		border-radius: 8px;
		background: var(--raised);
		box-shadow:
			inset 0 0 0 1px var(--line-strong),
			0 8px 24px var(--scrim-soft);
		color: var(--text);
		font-size: 13px;
		font-weight: 400;
		line-height: 1.5;
		transform-origin: top left;
		animation: pop-in 140ms ease-out;
	}
</style>

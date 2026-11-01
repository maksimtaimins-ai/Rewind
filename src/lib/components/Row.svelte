<!-- One setting: label with a one-line explanation below it on the left, control on the right. -->
<script lang="ts">
	import type { Snippet } from 'svelte';
	let { label, hint, children }: { label: string; hint?: string; children: Snippet } = $props();
</script>

<div class="row">
	<div class="copy">
		<div class="label">{label}</div>
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
</style>

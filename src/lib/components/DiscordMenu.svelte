<!-- "Post to Discord" button listing the webhooks set up in Settings > Sharing. -->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { Clip } from '#lib/api.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { postToDiscord } from '#lib/share.ts';

	let { clip }: { clip: Clip } = $props();

	const hooks = $derived(app.settings?.sharing.discord_webhooks ?? []);
	let open = $state(false);
	let busy = $state(false);
	let root = $state<HTMLDivElement>();

	async function pick(i: number) {
		open = false;
		busy = true;
		try {
			await postToDiscord(clip, i);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:window
	onpointerdown={(e) => open && !root?.contains(e.target as Node) && (open = false)}
	onkeydown={(e) => open && e.key === 'Escape' && (open = false)}
/>

<div class="wrap" bind:this={root}>
	<button class="secondary" disabled={busy} aria-haspopup="menu" aria-expanded={open} onclick={() => (open = !open)}>
		{busy ? 'Posting…' : 'Post to Discord'}
	</button>
	{#if open}
		<div class="menu" role="menu">
			{#each hooks as w, i (i)}
				<button role="menuitem" onclick={() => pick(i)}>
					<span class="name">{w.name || 'Unnamed webhook'}</span>
					<span class="what">Up to {w.limit_mb} MB{clip.size > w.limit_mb * 1e6 ? '. This clip needs a smaller copy first.' : ''}</span>
				</button>
			{:else}
				<button role="menuitem" onclick={() => goto('/settings#sharing')}>
					<span class="name">Add a webhook</span>
					<span class="what">Paste a channel's webhook URL in Settings.</span>
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
	.what {
		color: var(--muted);
		font-size: 12px;
	}
</style>

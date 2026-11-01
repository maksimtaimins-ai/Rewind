<!-- "Send to…" button listing other Rewinds on the LAN, with transfer progress. -->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { listen } from '@tauri-apps/api/event';
	import { onMount } from 'svelte';
	import { api, type Clip, type LanPeer, type LanProgress } from '#lib/api.ts';
	import { app, inApp } from '#lib/stores/app.svelte.ts';
	import { sendToPeer } from '#lib/share.ts';

	let { clip }: { clip: Clip } = $props();

	let open = $state(false);
	let peers = $state<LanPeer[]>([]);
	let looking = $state(false);
	let sending = $state(false);
	let pct = $state(0);
	let root = $state<HTMLDivElement>();
	const enabled = $derived(app.settings?.sharing.lan_enabled ?? false);

	onMount(() => {
		if (!inApp) return;
		const off = listen<LanProgress>('lan-progress', (e) => {
			if (e.payload.direction === 'out' && e.payload.path === clip.path) pct = Math.round((e.payload.done / e.payload.total) * 100);
		});
		return () => void off.then((f) => f());
	});

	async function toggle() {
		open = !open;
		if (!open || !enabled) return;
		if (!inApp) {
			peers = [{ id: 'laptop', name: 'LAPTOP-7Q2', addr: '192.168.1.20:7777' }];
			return;
		}
		looking = true;
		try {
			peers = await api.lanPeers();
		} finally {
			looking = false;
		}
	}

	async function pick(p: LanPeer) {
		open = false;
		sending = true;
		pct = 0;
		try {
			await sendToPeer(clip, p);
		} finally {
			sending = false;
		}
	}
</script>

<svelte:window
	onpointerdown={(e) => open && !root?.contains(e.target as Node) && (open = false)}
	onkeydown={(e) => open && e.key === 'Escape' && (open = false)}
/>

<div class="wrap" bind:this={root}>
	<button class="secondary" disabled={sending} aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
		{sending ? `Sending ${pct}%` : 'Send to…'}
	</button>
	{#if open}
		<div class="menu" role="menu">
			{#if !enabled}
				<button role="menuitem" onclick={() => goto('/settings#sharing')}>
					<span class="name">Turn on LAN send</span>
					<span class="what">Send clips to other PCs running Rewind on this network.</span>
				</button>
			{:else if looking}
				<p class="what pad">Looking for PCs…</p>
			{:else}
				{#each peers as p (p.id)}
					<button role="menuitem" onclick={() => pick(p)}>
						<span class="name">{p.name}</span>
						<span class="what num">{p.addr}</span>
					</button>
				{:else}
					<p class="what pad">No other PCs found. Rewind must be running with LAN send on, on the same network.</p>
				{/each}
			{/if}
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
		border-radius: 6px;
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
	.pad {
		margin: 0;
		padding: 8px 12px;
	}
</style>

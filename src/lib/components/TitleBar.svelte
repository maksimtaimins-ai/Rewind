<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { inApp } from '#lib/stores/app.svelte.ts';
	import { openSearch } from '#lib/stores/search.svelte.ts';

	let { title = 'Rewind', search = true }: { title?: string; search?: boolean } = $props();
	const win = inApp ? getCurrentWindow() : null;
</script>

<header class="bar" data-tauri-drag-region>
	<span class="title" data-tauri-drag-region>{title}</span>
	{#if search}
		<button class="find" onclick={openSearch} aria-label="Search clips, settings and help" aria-keyshortcuts="Control+K">
			<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" /></svg>
			<span>Search</span>
			<kbd>Ctrl K</kbd>
		</button>
	{/if}
	<div class="controls">
		<button aria-label="Minimize" onclick={() => win?.minimize()}>
			<svg viewBox="0 0 10 10"><path d="M1 5h8" /></svg>
		</button>
		<button aria-label="Maximize" onclick={() => win?.toggleMaximize()}>
			<svg viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" /></svg>
		</button>
		<button class="close" aria-label="Close to tray" title="Close to tray (keeps recording)" onclick={() => win?.hide()}>
			<svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
		</button>
	</div>
</header>

<style>
	.bar {
		height: 32px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-left: 16px;
		flex: none;
	}
	.title {
		font-size: 12px;
		color: var(--faint);
		font-weight: 500;
	}
	@media (max-width: 559px) {
		.title,
		.find span,
		.find kbd {
			display: none;
		}
		.find {
			width: 28px;
			justify-content: center;
			padding: 0;
		}
	}
	@media (max-width: 399px) {
		.find {
			display: none;
		}
	}
	.controls {
		display: flex;
		height: 100%;
	}
	button {
		border: 0;
		cursor: pointer;
	}
	.find {
		position: absolute;
		left: 50%;
		translate: -50% 0;
		width: min(320px, 40vw);
		height: 24px;
		top: 4px;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 8px;
		border-radius: 8px;
		background: var(--sunken);
		color: var(--faint);
		font-size: 12px;
		transition: color 180ms ease-out, background-color 180ms ease-out;
	}
	.find span {
		flex: 1;
		text-align: left;
	}
	.find:hover {
		background: var(--raised);
		color: var(--text);
	}
	.find svg {
		width: 12px;
		height: 12px;
		stroke-width: 1.5;
		stroke-linecap: round;
	}
	kbd {
		font: 500 10px var(--font-sans);
		color: var(--faint);
	}
	.bar {
		position: relative;
	}
	.controls button {
		width: 44px;
		height: 100%;
		display: grid;
		place-items: center;
		background: transparent;
		border: 0;
		color: var(--muted);
	}
	button:hover {
		background: var(--raised);
		color: var(--text);
	}
	.close:hover {
		background: var(--danger);
		color: var(--on-danger);
	}
	svg {
		width: 10px;
		height: 10px;
		fill: none;
		stroke: currentColor;
		stroke-width: 1;
	}
</style>

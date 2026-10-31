<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { inApp } from '#lib/stores/app.svelte.ts';

	let { title = 'Rewind' }: { title?: string } = $props();
	const win = inApp ? getCurrentWindow() : null;
</script>

<header class="bar" data-tauri-drag-region>
	<span class="title" data-tauri-drag-region>{title}</span>
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
	.controls {
		display: flex;
		height: 100%;
	}
	button {
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

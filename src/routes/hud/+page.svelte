<!-- Compact HUD: the floating pill shown instead of the main window. -->
<script lang="ts">
	import { api } from '#lib/api.ts';
	import { app, inApp, toast } from '#lib/stores/app.svelte.ts';

	const st = $derived(app.status);
	const held = $derived(Math.min(st?.buffered_secs ?? 0, st?.buffer_secs ?? 0));
	let saving = $state(false);

	async function save() {
		if (!inApp) return;
		saving = true;
		try {
			await api.saveNow();
		} catch (e) {
			toast('error', String(e));
		} finally {
			setTimeout(() => (saving = false), 600);
		}
	}

	function menu(e: MouseEvent) {
		e.preventDefault();
		if (inApp) api.hudMenu();
	}
</script>

<svelte:head><title>Rewind HUD</title></svelte:head>

<div class="pill" data-tauri-drag-region oncontextmenu={menu} role="toolbar" aria-label="Rewind" tabindex="-1">
	<span class="dot" aria-hidden="true" data-tauri-drag-region></span>
	<span class="buf" data-tauri-drag-region title={st?.state === 'paused' ? 'Paused while a private app is focused' : undefined}>
		Buffer: <span class="num">{held.toFixed(0)}s / {st?.buffer_secs ?? 0}s</span>
	</span>
	<button class="save" disabled={saving} onclick={save} title="Save clip{app.settings?.hotkeys.save_clip ? ` (${app.settings.hotkeys.save_clip})` : ''}">
		{saving ? 'Saved' : 'Save'}
	</button>
	<button class="icon" onclick={() => inApp && api.setCompactHud(false)} aria-label="Full window" title="Full window">
		<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7" /></svg>
	</button>
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: hidden;
	}
	.pill {
		box-sizing: border-box;
		height: 56px;
		width: 256px;
		padding: 0 8px 0 16px;
		display: flex;
		align-items: center;
		gap: 8px;
		border-radius: 28px;
		background: var(--raised);
		box-shadow: inset 0 0 0 1px var(--line-strong);
		user-select: none;
		cursor: grab;
	}
	.dot {
		width: 10px;
		height: 10px;
		flex: none;
		border-radius: 50%;
		background: #e5484d;
		box-shadow: 0 0 0 3px color-mix(in srgb, #e5484d 25%, transparent);
	}
	.buf {
		flex: 1 0 auto;
		font-size: 13px;
		white-space: nowrap;
	}
	.save {
		height: 32px;
		padding: 0 10px;
		border: 0;
		border-radius: 12px;
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
		cursor: pointer;
	}
	.save:disabled {
		opacity: 0.7;
	}
	.icon {
		width: 32px;
		height: 32px;
		flex: none;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 12px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.icon:hover {
		background: var(--line);
		color: var(--text);
	}
	.icon svg {
		width: 16px;
		height: 16px;
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>

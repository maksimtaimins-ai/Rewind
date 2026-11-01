<!-- Mini scrubber for choosing a clip's thumbnail frame. -->
<script lang="ts">
	import { fileUrl, fmtDuration } from '#lib/api.ts';
	import { inApp } from '#lib/stores/app.svelte.ts';

	let {
		path,
		duration,
		start = 0,
		onpick,
		onclose
	}: { path: string; duration: number; start?: number; onpick: (secs: number) => Promise<void>; onclose: () => void } = $props();

	let video = $state<HTMLVideoElement>();
	let t = $state(0);
	let saving = $state(false);
	let dialog = $state<HTMLDialogElement>();

	$effect(() => {
		dialog?.showModal();
		t = start;
	});

	function seek(v: number) {
		t = Math.min(duration, Math.max(0, v));
		if (video) video.currentTime = t;
	}

	async function use() {
		saving = true;
		try {
			await onpick(t);
			onclose();
		} finally {
			saving = false;
		}
	}
</script>

<dialog bind:this={dialog} onclose={onclose} aria-labelledby="thumb-h">
	<h2 id="thumb-h">Pick a thumbnail</h2>
	<div class="frame">
		{#if inApp}
			<!-- svelte-ignore a11y_media_has_caption -->
			<video bind:this={video} src={fileUrl(path)} preload="auto" muted onloadedmetadata={() => seek(t)}></video>
		{:else}
			<span class="ph">Frame preview shows here inside the app.</span>
		{/if}
	</div>
	<div class="scrub">
		<button class="step" aria-label="Previous frame" onclick={() => seek(t - 1 / 60)}>‹</button>
		<input type="range" min="0" max={duration} step="0.001" value={t} oninput={(e) => seek(Number(e.currentTarget.value))} aria-label="Thumbnail frame time" />
		<button class="step" aria-label="Next frame" onclick={() => seek(t + 1 / 60)}>›</button>
		<span class="num time">{fmtDuration(t)}.{String(Math.floor((t % 1) * 100)).padStart(2, '0')}</span>
	</div>
	<div class="buttons">
		<button class="secondary" onclick={() => dialog?.close()}>Cancel</button>
		<button class="primary" disabled={saving || !inApp} onclick={use}>{saving ? 'Saving…' : 'Use this frame'}</button>
	</div>
</dialog>

<style>
	dialog {
		width: min(640px, calc(100vw - 64px));
		/* Preflight zeroes margins; modal dialogs centre with auto. */
		margin: auto;
		padding: 20px;
		border: 0;
		border-radius: 8px;
		background: var(--raised);
		color: var(--text);
		box-shadow: inset 0 0 0 1px var(--line-strong);
	}
	dialog::backdrop {
		background: var(--scrim);
	}
	h2 {
		margin: 0 0 12px;
		font-size: 18px;
		font-weight: 600;
	}
	.frame {
		aspect-ratio: 16 / 9;
		border-radius: 4px;
		overflow: hidden;
		background: var(--media);
		display: grid;
		place-items: center;
	}
	video {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
	.ph {
		color: var(--faint);
	}
	.scrub {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 12px;
	}
	input[type='range'] {
		flex: 1;
		accent-color: var(--accent);
	}
	.step {
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		font-size: 16px;
		cursor: pointer;
	}
	.step:hover {
		background: var(--line);
	}
	.time {
		width: 64px;
		text-align: right;
		color: var(--muted);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}
	.primary,
	.secondary {
		height: 36px;
		padding: 0 16px;
		border: 0;
		border-radius: 4px;
		font-weight: 600;
		cursor: pointer;
	}
	.primary {
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled {
		background: var(--line);
		color: var(--faint);
		cursor: default;
	}
	.secondary {
		background: var(--sunken);
	}
	.secondary:hover {
		background: var(--line);
	}
</style>

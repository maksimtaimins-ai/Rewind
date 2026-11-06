<script lang="ts">
	import type { Status } from '#lib/api.ts';
	import { fmtDuration } from '#lib/api.ts';

	let { status, compact = false }: { status: Status | null; compact?: boolean } = $props();

	const line = $derived.by(() => {
		if (!status) return 'Starting capture';
		switch (status.state) {
			case 'recording':
				return `Buffering · ${fmtDuration(Math.min(status.buffered_secs, status.buffer_secs))} held`;
			case 'paused':
				return status.manual_pause ? 'Buffer paused' : 'Paused while a private app is focused';
			case 'error':
				return status.error ?? 'Capture failed';
			default:
				return 'Starting capture';
		}
	});
</script>

<div class="status" class:compact title={status?.error ?? line}>
	<span class="dot {status?.state ?? 'idle'}" aria-hidden="true"></span>
	<span class="text">{line}</span>
</div>

<style>
	.status {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		color: var(--muted);
	}
	.text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--faint);
	}
	.dot.recording {
		background: var(--signal);
		animation: breathe 2s ease-in-out infinite;
	}
	.dot.paused {
		background: transparent;
		box-shadow: inset 0 0 0 2px var(--muted);
	}
	.dot.error {
		background: var(--text);
	}
	@keyframes breathe {
		0%,
		100% {
			opacity: 0.6;
		}
		50% {
			opacity: 1;
		}
	}
</style>

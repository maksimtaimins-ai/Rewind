<!-- Markers set this session. Each row turns its marker into a clip of a chosen length. -->
<script lang="ts">
	import { fmtDuration, type MarkerInfo } from '#lib/api.ts';
	import Segmented from './Segmented.svelte';

	let {
		markers,
		maxSecs,
		onclip
	}: { markers: MarkerInfo[]; maxSecs: number; onclip: (id: number, secs: number) => void } = $props();

	type Choice = 10 | 30 | 60 | 'custom';
	let choice = $state<Record<number, Choice>>({});
	let custom = $state<Record<number, number>>({});

	const lengthOf = (id: number) => {
		const c = choice[id] ?? 30;
		return Math.min(maxSecs, Math.max(1, c === 'custom' ? (custom[id] ?? 20) : c));
	};
	const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
	const ago = (s: number) => (s < 60 ? `${Math.round(s)}s ago` : `${fmtDuration(s)} ago`);
	// Newest first: the moment you just marked is the one you want.
	const rows = $derived([...markers].reverse());
</script>

<ul class="list">
	{#each rows as m (m.id)}
		<li class:gone={!m.in_buffer}>
			<span class="when"><span class="num">{clock(m.at_ms)}</span><span class="ago">{ago(m.ago)}</span></span>
			{#if m.in_buffer}
				<span class="pick">
					<Segmented
						label="Clip length"
						value={choice[m.id] ?? 30}
						options={[
							{ value: 10, label: '10s' },
							{ value: 30, label: '30s' },
							{ value: 60, label: '60s' },
							{ value: 'custom', label: 'Custom' }
						]}
						onchange={(v) => (choice[m.id] = v)}
					/>
					{#if choice[m.id] === 'custom'}
						<input
							class="num"
							type="number"
							min="1"
							max={maxSecs}
							value={custom[m.id] ?? 20}
							onchange={(e) => (custom[m.id] = Number((e.currentTarget as HTMLInputElement).value))}
							aria-label="Custom clip length in seconds"
						/>
					{/if}
				</span>
				<button onclick={() => onclip(m.id, lengthOf(m.id))}>Make clip</button>
			{:else}
				<span class="warn">Older than the buffer. Its footage is gone.</span>
			{/if}
		</li>
	{/each}
</ul>

<style>
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 8px 0;
		border-top: 1px solid var(--line);
	}
	.when {
		display: flex;
		align-items: baseline;
		gap: 12px;
		min-width: 168px;
	}
	.ago {
		color: var(--muted);
		font-size: 13px;
	}
	.gone .when {
		color: var(--faint);
	}
	.pick {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 1;
	}
	input {
		width: 64px;
		height: 28px;
		padding: 0 8px;
		text-align: right;
		border: 1px solid var(--line);
		border-radius: 4px;
		background: var(--ground);
		outline: none;
	}
	input:focus {
		border-color: var(--muted);
	}
	button {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 4px;
		background: var(--raised);
		font-weight: 500;
		cursor: pointer;
	}
	button:hover {
		background: var(--line);
	}
	.warn {
		color: var(--faint);
		font-size: 13px;
	}
</style>

<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { open } from '@tauri-apps/plugin-dialog';
	import { api, type BenchResult, type EncoderChoice } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import HotkeyInput from '#lib/components/HotkeyInput.svelte';
	import Segmented from '#lib/components/Segmented.svelte';

	const s = $derived(app.settings!);
	const steps = ['Encoder', 'Hotkey', 'Clips folder', 'Ready'];
	let step = $state(0);
	let results = $state<BenchResult[] | null>(null);
	let testing = $state(false);
	let failed = $state<string | null>(null);

	const kindName: Record<string, string> = {
		nvenc: 'NVIDIA NVENC',
		amf: 'AMD AMF',
		qsv: 'Intel Quick Sync',
		other: 'Hardware encoder',
		software: 'CPU encoder'
	};

	// Best working encoder for H.264: hardware first, then fastest.
	const best = $derived.by(() => {
		const ok = (results ?? []).filter((r) => r.ok && r.codec === 'h264');
		const order = ['nvenc', 'amf', 'qsv', 'other', 'software'];
		return ok.sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind) || b.fps - a.fps)[0] ?? null;
	});
	const unique = $derived(
		(results ?? []).filter((r, i, all) => all.findIndex((x) => x.name === r.name && x.codec === r.codec) === i)
	);

	async function test() {
		testing = true;
		failed = null;
		try {
			results = inApp
				? await api.benchmark(5)
				: [
						{ kind: 'nvenc', name: 'NVIDIA H.264 Encoder MFT', codec: 'h264', fps: 312, ok: true, error: null },
						{ kind: 'qsv', name: 'Intel Quick Sync Video H.264 Encoder MFT', codec: 'h264', fps: 34, ok: true, error: null },
						{ kind: 'software', name: 'Microsoft H.264 (CPU)', codec: 'h264', fps: 36, ok: true, error: null }
					];
		} catch (e) {
			failed = String(e);
		} finally {
			testing = false;
		}
	}

	onMount(test);

	async function useBest() {
		if (best) await updateSettings((x) => (x.video.encoder = best.kind === 'other' ? 'auto' : (best.kind as EncoderChoice)));
		step = 1;
	}

	async function pickFolder() {
		if (!inApp) return;
		const dir = await open({ directory: true, defaultPath: s.clips_dir, title: 'Save clips to' });
		if (typeof dir === 'string') updateSettings((x) => (x.clips_dir = dir));
	}

	async function finish() {
		await updateSettings((x) => (x.first_run_complete = true));
		goto('/');
	}
</script>

<div class="wizard">
	<ol class="steps" aria-label="Setup steps">
		{#each steps as label, i (label)}
			<li class:done={i < step} class:now={i === step} aria-current={i === step ? 'step' : undefined}>
				<span class="n num">{i + 1}</span>{label}
			</li>
		{/each}
	</ol>

	<div class="panel">
		{#if step === 0}
			<h1>Finding your encoder</h1>
			<p class="lede">
				Rewind encodes on your graphics card so games keep their frame rate. This test encodes a few seconds of
				frames on each encoder it finds.
			</p>
			{#if testing}
				<p class="working">Testing encoders…</p>
			{:else if failed}
				<p class="problem">The test couldn't run: {failed}</p>
				<button class="secondary" onclick={test}>Try again</button>
			{:else if results}
				<table>
					<tbody>
						{#each unique as r, i (i)}
							<tr class:pick={best && r.name === best.name && r.codec === best.codec}>
								<td>{kindName[r.kind]}<span class="sub">{r.name}, {r.codec === 'h264' ? 'H.264' : 'HEVC'}</span></td>
								<td class="num">{r.ok ? `${r.fps.toFixed(0)} fps` : 'Not usable'}</td>
							</tr>
						{/each}
					</tbody>
				</table>
				{#if best && best.kind === 'software'}
					<p class="problem">
						No hardware encoder worked. The CPU encoder will cost frames while gaming; updating your graphics driver
						usually fixes this.
					</p>
				{/if}
			{/if}
			<div class="nav">
				<span></span>
				<button class="primary" disabled={testing || !best} onclick={useBest}>
					{best ? `Use ${kindName[best.kind]}` : 'Continue'}
				</button>
			</div>
		{:else if step === 1}
			<h1>Pick your save key</h1>
			<p class="lede">Press it any time while playing and the last moments are written to disk.</p>
			<div class="field">
				<span>Save clip</span>
				<HotkeyInput label="Save clip" value={s.hotkeys.save_clip} onchange={(v) => updateSettings((x) => (x.hotkeys.save_clip = v))} />
			</div>
			<div class="field">
				<span>How far back</span>
				<Segmented
					label="Replay length"
					value={s.buffer.seconds}
					options={[
						{ value: 10, label: '10s' },
						{ value: 30, label: '30s' },
						{ value: 60, label: '1m' },
						{ value: 120, label: '2m' },
						{ value: 300, label: '5m' }
					]}
					onchange={(v) => updateSettings((x) => (x.buffer.seconds = v))}
				/>
			</div>
			<div class="nav">
				<button class="secondary" onclick={() => (step = 0)}>Back</button>
				<button class="primary" disabled={!s.hotkeys.save_clip} onclick={() => (step = 2)}>Continue</button>
			</div>
		{:else if step === 2}
			<h1>Where clips go</h1>
			<p class="lede">Clips stay on this PC. Nothing is uploaded, and there's no account.</p>
			<div class="field">
				<span class="path" title={s.clips_dir}>{s.clips_dir}</span>
				<button class="secondary" onclick={pickFolder}>Change</button>
			</div>
			<div class="nav">
				<button class="secondary" onclick={() => (step = 1)}>Back</button>
				<button class="primary" onclick={() => (step = 3)}>Continue</button>
			</div>
		{:else}
			<h1>Ready</h1>
			<p class="lede">
				The replay buffer is already running and keeps the last {s.buffer.seconds} seconds in memory while Rewind is open.
				Press {s.hotkeys.save_clip} any time to save them. Nothing touches the disk until you do.
			</p>
			<div class="nav">
				<button class="secondary" onclick={() => (step = 2)}>Back</button>
				<button class="primary" onclick={finish}>Start using Rewind</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.wizard {
		display: grid;
		grid-template-columns: 200px minmax(0, 560px);
		gap: 48px;
		padding: 32px 48px;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 12px;
		align-content: start;
		color: var(--faint);
	}
	.steps li {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.n {
		width: 24px;
		height: 24px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		font-size: 11px;
	}
	.steps li.now {
		color: var(--text);
	}
	.steps li.now .n {
		border-color: var(--text);
	}
	.steps li.done {
		color: var(--muted);
	}
	.steps li.done .n {
		background: var(--line-strong);
		border-color: var(--line-strong);
	}
	h1 {
		margin: 0;
		font-size: 32px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.lede {
		color: var(--muted);
		margin: 8px 0 24px;
		max-width: 52ch;
	}
	.working {
		color: var(--muted);
	}
	.problem {
		color: var(--text);
		max-width: 52ch;
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	td {
		padding: 12px 0;
		border-top: 1px solid var(--line);
		vertical-align: top;
	}
	td:last-child {
		text-align: right;
		color: var(--muted);
	}
	tr.pick td {
		font-weight: 600;
	}
	tr.pick td:last-child {
		color: var(--text);
	}
	.sub {
		display: block;
		color: var(--faint);
		font-weight: 400;
		font-size: 12px;
	}
	.field {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 12px 0;
		border-top: 1px solid var(--line);
	}
	.path {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--muted);
	}
	.nav {
		display: flex;
		justify-content: space-between;
		margin-top: 32px;
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
		background: var(--raised);
	}
</style>

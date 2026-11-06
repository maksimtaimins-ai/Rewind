<!--
  Settings → Macros: a hotkey that runs a chain of actions, in order. Up to
  ten. Nothing runs, and no extra key is registered, until one is on.
-->
<script lang="ts">
	import { api, type ExportPreset, type MacroStep, type Settings } from '#lib/api.ts';
	import { app, inApp, updateSettings } from '#lib/stores/app.svelte.ts';
	import HotkeyInput from './HotkeyInput.svelte';
	import Toggle from './Toggle.svelte';

	const MAX = 10;
	const MAX_STEPS = 12;
	type M = Settings['macros'][number];

	const macros = $derived(app.settings!.macros);
	const hooks = $derived(app.settings!.sharing.discord_webhooks);
	const set = (f: (m: M[]) => void) => updateSettings((x) => f(x.macros));

	const library: { kind: MacroStep['kind']; label: string }[] = [
		{ kind: 'save_clip', label: 'Save clip' },
		{ kind: 'set_marker', label: 'Set marker' },
		{ kind: 'screenshot', label: 'Take screenshot' },
		{ kind: 'slow_mo', label: 'Slow-mo replay' },
		{ kind: 'extend_last', label: 'Extend last clip' },
		{ kind: 'open_last_clip', label: 'Open last clip' },
		{ kind: 'toggle_hud', label: 'Toggle compact HUD' },
		{ kind: 'pause_buffer', label: 'Pause / resume buffer' },
		{ kind: 'run_export', label: 'Run export preset' },
		{ kind: 'post_discord', label: 'Post to Discord' },
		{ kind: 'show_toast', label: 'Show a toast' }
	];
	const label = (k: MacroStep['kind']) => library.find((l) => l.kind === k)!.label;
	const presets: { value: ExportPreset; label: string }[] = [
		{ value: 'discord10', label: 'Discord 10 MB' },
		{ value: 'discord25', label: 'Discord 25 MB' },
		{ value: 'shorts', label: 'Shorts / vertical' },
		{ value: 'twitter', label: 'Twitter / X' },
		{ value: 'gif', label: 'GIF' }
	];
	const needsClip = new Set(['open_last_clip', 'run_export', 'post_discord']);

	function fresh(kind: MacroStep['kind']): MacroStep {
		switch (kind) {
			case 'run_export':
				return { kind, preset: 'discord10' };
			case 'post_discord':
				return { kind, webhook: 0 };
			case 'show_toast':
				return { kind, message: 'Done' };
			default:
				return { kind } as MacroStep;
		}
	}

	const addMacro = () =>
		set((m) => m.push({ name: `Macro ${m.length + 1}`, hotkey: '', enabled: true, steps: [{ kind: 'save_clip' }, { kind: 'show_toast', message: 'Clip saved' }] }));

	function move(m: M, i: number, by: number) {
		const j = i + by;
		if (j < 0 || j >= m.steps.length) return;
		[m.steps[i], m.steps[j]] = [m.steps[j], m.steps[i]];
	}

	/** A step that needs a clip, with no save before it, works on the last one saved earlier. */
	const earlyClip = (m: M, i: number) => needsClip.has(m.steps[i].kind) && !m.steps.slice(0, i).some((s) => ['save_clip', 'slow_mo', 'extend_last'].includes(s.kind));
</script>

<p class="intro">
	One hotkey, several actions, run in order. A step that needs a clip (post, export, open) uses the one just saved, and waits for a save in the same
	macro to finish first. Up to {MAX} macros; none configured means none of this runs.
</p>

{#each macros as m, mi (mi)}
	<div class="macro" class:off={!m.enabled}>
		<div class="head">
			<input class="name" type="text" value={m.name} maxlength="40" onchange={(e) => set((l) => (l[mi].name = e.currentTarget.value.trim() || `Macro ${mi + 1}`))} aria-label="Macro name" />
			<HotkeyInput label="{m.name} hotkey" value={m.hotkey} onchange={(v) => set((l) => (l[mi].hotkey = v))} />
			<button class="ghost" disabled={!inApp || !m.enabled} title="Run it now" onclick={() => api.runMacro(mi).catch(() => {})}>Run</button>
			<Toggle checked={m.enabled} label="Macro {m.name} on" onchange={(v) => set((l) => (l[mi].enabled = v))} />
		</div>
		<ol class="steps">
			{#each m.steps as st, si (si)}
				<li>
					<span class="n num">{si + 1}</span>
					<span class="what">{label(st.kind)}</span>
					{#if st.kind === 'run_export'}
						<select value={st.preset} onchange={(e) => set((l) => ((l[mi].steps[si] as Extract<MacroStep, { kind: 'run_export' }>).preset = e.currentTarget.value as ExportPreset))} aria-label="Export preset">
							{#each presets as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
						</select>
					{:else if st.kind === 'post_discord'}
						{#if hooks.length}
							<select value={st.webhook} onchange={(e) => set((l) => ((l[mi].steps[si] as Extract<MacroStep, { kind: 'post_discord' }>).webhook = Number(e.currentTarget.value)))} aria-label="Discord webhook">
								{#each hooks as w, wi (wi)}<option value={wi}>{w.name || 'Discord'}</option>{/each}
							</select>
						{:else}
							<span class="warn">Add a webhook in Sharing first</span>
						{/if}
					{:else if st.kind === 'show_toast'}
						<input class="msg" type="text" value={st.message} maxlength="120" onchange={(e) => set((l) => ((l[mi].steps[si] as Extract<MacroStep, { kind: 'show_toast' }>).message = e.currentTarget.value))} aria-label="Toast message" />
					{/if}
					{#if earlyClip(m, si)}<span class="note" title="No save comes before this step, so it uses the last clip you saved.">last clip</span>{/if}
					<span class="tools">
						<button aria-label="Move up" disabled={si === 0} onclick={() => set((l) => move(l[mi], si, -1))}>↑</button>
						<button aria-label="Move down" disabled={si === m.steps.length - 1} onclick={() => set((l) => move(l[mi], si, 1))}>↓</button>
						<button aria-label="Remove step" onclick={() => set((l) => l[mi].steps.splice(si, 1))}>×</button>
					</span>
				</li>
			{/each}
		</ol>
		<div class="foot">
			<select
				aria-label="Add a step"
				disabled={m.steps.length >= MAX_STEPS}
				onchange={(e) => {
					const kind = e.currentTarget.value as MacroStep['kind'];
					e.currentTarget.value = '';
					if (kind) set((l) => l[mi].steps.push(fresh(kind)));
				}}
			>
				<option value="">Add a step…</option>
				{#each library as l (l.kind)}<option value={l.kind}>{l.label}</option>{/each}
			</select>
			<button class="ghost" onclick={() => set((l) => l.splice(mi, 1))}>Delete macro</button>
		</div>
	</div>
{/each}

<div class="add">
	<button class="secondary" disabled={macros.length >= MAX} onclick={addMacro}>Add a macro</button>
	{#if macros.length >= MAX}<span class="note">Ten is the limit.</span>{/if}
</div>

<style>
	.intro {
		color: var(--muted);
		font-size: 13px;
		margin: 0 0 12px;
		max-width: 62ch;
	}
	.macro {
		padding: 12px;
		margin-bottom: 12px;
		border-radius: var(--r-2);
		background: var(--sunken);
		display: grid;
		gap: 10px;
	}
	.macro.off .steps {
		opacity: 0.55;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.name {
		flex: 1;
		min-width: 120px;
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--raised);
		font-weight: 600;
		outline: none;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 34px;
		padding: 0 6px 0 10px;
		border-radius: var(--r-1);
		background: var(--raised);
	}
	.n {
		color: var(--faint);
		width: 14px;
	}
	.what {
		font-weight: 500;
	}
	.msg {
		flex: 1;
		min-width: 0;
		height: 26px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		outline: none;
	}
	.tools {
		margin-left: auto;
		display: flex;
	}
	.tools button {
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: 4px;
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.tools button:hover:not(:disabled) {
		color: var(--text);
		background: var(--line);
	}
	.tools button:disabled {
		opacity: 0.3;
		cursor: default;
	}
	.foot,
	.add {
		display: flex;
		align-items: center;
		gap: 10px;
		justify-content: space-between;
	}
	.add {
		justify-content: flex-start;
	}
	select {
		height: 28px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
	}
	.foot select {
		background: var(--raised);
	}
	.secondary,
	.ghost {
		height: 32px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--r-1);
		font-weight: 500;
		cursor: pointer;
	}
	.secondary {
		background: var(--raised);
	}
	.ghost {
		background: transparent;
		color: var(--muted);
	}
	.ghost:hover:not(:disabled),
	.secondary:hover:not(:disabled) {
		color: var(--text);
		background: var(--line);
	}
	.ghost:disabled,
	.secondary:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.note {
		color: var(--faint);
		font-size: 12px;
	}
	.warn {
		color: var(--muted);
		font-size: 12px;
	}
</style>

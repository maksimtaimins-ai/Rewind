<!--
  Settings → Library → File names: the clip name template, with a live preview.
-->
<script lang="ts">
	import { DEFAULT_CLIP_TEMPLATE, NAME_TOKENS, renderName, sample } from '#lib/naming.ts';
	import { app, updateSettings } from '#lib/stores/app.svelte.ts';
	import Row from './Row.svelte';

	const template = $derived(app.settings!.naming.clip_template);
	let input = $state<HTMLInputElement>();
	let draft = $state('');
	$effect(() => {
		draft = template;
	});

	const ctx = sample();
	const preview = $derived(renderName(draft.trim() || DEFAULT_CLIP_TEMPLATE, ctx));
	const save = (v: string) => updateSettings((x) => (x.naming.clip_template = v.trim() || DEFAULT_CLIP_TEMPLATE));

	function insert(token: string) {
		const at = input?.selectionStart ?? draft.length;
		const end = input?.selectionEnd ?? at;
		draft = `${draft.slice(0, at)}{${token}}${draft.slice(end)}`;
		save(draft);
		queueMicrotask(() => {
			input?.focus();
			const pos = at + token.length + 2;
			input?.setSelectionRange(pos, pos);
		});
	}
</script>

<Row id="s-clip-name" label="Clip names" hint="Tokens in braces are filled in when a clip is saved. Renaming a clip later never changes the template.">
	<div class="tpl">
		<input type="text" class="field" bind:this={input} bind:value={draft} maxlength="120" spellcheck="false" aria-label="Clip name template" onchange={() => save(draft)} />
		<button class="reset" disabled={template === DEFAULT_CLIP_TEMPLATE} onclick={() => save(DEFAULT_CLIP_TEMPLATE)}>Reset</button>
	</div>
</Row>
<div class="under">
	<div class="chips" role="group" aria-label="Insert a token">
		{#each NAME_TOKENS as t (t)}
			<button class="chip" onclick={() => insert(t)}>{`{${t}}`}</button>
		{/each}
	</div>
	<p class="preview"><span class="k">Looks like</span> <span class="num">{preview}.mp4</span></p>
</div>

<style>
	.tpl {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.field {
		width: 260px;
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--r-1);
		background: var(--sunken);
		outline: none;
		font-family: var(--mono, ui-monospace, monospace);
		font-size: 13px;
	}
	.field:focus-visible {
		box-shadow: 0 0 0 2px var(--accent);
	}
	.reset {
		height: 32px;
		padding: 0 10px;
		border: 0;
		border-radius: var(--r-1);
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}
	.reset:hover:not(:disabled) {
		color: var(--text);
	}
	.reset:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.under {
		padding-bottom: var(--s-3);
		display: grid;
		gap: 8px;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.chip {
		height: 24px;
		padding: 0 8px;
		border: 0;
		border-radius: 6px;
		background: var(--sunken);
		color: var(--muted);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.chip:hover {
		color: var(--text);
		background: var(--line);
	}
	.preview {
		margin: 0;
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.k {
		color: var(--faint);
		margin-right: 6px;
	}
</style>

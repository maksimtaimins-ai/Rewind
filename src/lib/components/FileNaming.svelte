<!--
  Settings → Library → File names: the clip name template, with a live preview.
-->
<script lang="ts">
	import { DEFAULT_CLIP_TEMPLATE, FOLDER_TOKENS, NAME_TOKENS, folderTemplate, renderFolder, renderName, sample, type FolderMode } from '#lib/naming.ts';
	import Segmented from './Segmented.svelte';
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

	const nm = $derived(app.settings!.naming);
	let folderInput = $state<HTMLInputElement>();
	let folderDraft = $state('');
	$effect(() => {
		folderDraft = nm.folder_template;
	});
	const folderPath = $derived(renderFolder(folderTemplate(nm.folder_mode, folderDraft), ctx));
	const saveFolder = (v: string) => updateSettings((x) => (x.naming.folder_template = v.trim()));
	function insertFolder(token: string) {
		const at = folderInput?.selectionStart ?? folderDraft.length;
		const end = folderInput?.selectionEnd ?? at;
		folderDraft = `${folderDraft.slice(0, at)}{${token}}${folderDraft.slice(end)}`;
		saveFolder(folderDraft);
	}

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

<Row id="s-folders" label="Folders" hint="Sort new clips into subfolders of your save folder.">
	<Segmented
		label="Folder organization"
		value={nm.folder_mode}
		options={[
			{ value: 'flat' as FolderMode, label: 'Flat' },
			{ value: 'game' as FolderMode, label: 'Per game' },
			{ value: 'date' as FolderMode, label: 'Per date' },
			{ value: 'game_date' as FolderMode, label: 'Game + date' },
			{ value: 'custom' as FolderMode, label: 'Custom' }
		]}
		onchange={(v) => updateSettings((x) => (x.naming.folder_mode = v))}
	/>
</Row>
<div class="under">
	{#if nm.folder_mode === 'custom'}
		<input type="text" class="field wide" bind:this={folderInput} bind:value={folderDraft} maxlength="120" spellcheck="false" placeholder={'{game}/{year}-{month}'} aria-label="Folder template" onchange={() => saveFolder(folderDraft)} />
		<div class="chips" role="group" aria-label="Insert a folder token">
			{#each FOLDER_TOKENS as t (t)}
				<button class="chip" onclick={() => insertFolder(t)}>{`{${t}}`}</button>
			{/each}
		</div>
	{/if}
	<p class="preview">
		<span class="k">Saves to</span>
		<span class="num">{['…', ...folderPath, `${preview}.mp4`].join(' / ')}</span>
	</p>
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
	.field.wide {
		width: 100%;
		box-sizing: border-box;
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

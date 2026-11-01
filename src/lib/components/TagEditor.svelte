<!--
  Add tags to a clip. Typing a tag that has never been used asks for its
  colour once; known tags are suggested as you type.
-->
<script lang="ts">
	import { app } from '#lib/stores/app.svelte.ts';
	import { allTags } from '#lib/filter.ts';

	let {
		tags,
		onadd,
		autofocus = false
	}: { tags: string[]; onadd: (tag: string, color?: string) => void; autofocus?: boolean } = $props();

	// Tag colours are the user's labels, not theme colours: the same in every theme.
	const palette = ['#e5484d', '#f76b15', '#ffc53d', '#46a758', '#12a594', '#0090ff', '#8e4ec6', '#d6409f'];

	let text = $state('');
	let pending = $state<string | null>(null);
	let input = $state<HTMLInputElement>();

	$effect(() => {
		if (autofocus) input?.focus();
	});

	const known = $derived(allTags(app.clips).concat(Object.keys(app.meta.tag_colors)));
	const suggestions = $derived(
		text.trim()
			? [...new Set(known)]
					.filter((t) => t.toLowerCase().includes(text.trim().toLowerCase()) && !tags.some((x) => x.toLowerCase() === t.toLowerCase()))
					.slice(0, 6)
			: []
	);

	function submit(name = text) {
		const t = name.trim().slice(0, 32);
		if (!t || tags.some((x) => x.toLowerCase() === t.toLowerCase())) {
			text = '';
			return;
		}
		const existing = Object.keys(app.meta.tag_colors).find((k) => k.toLowerCase() === t.toLowerCase());
		if (existing) {
			onadd(existing);
			text = '';
		} else {
			pending = t;
		}
	}

	function pick(color: string) {
		if (!pending) return;
		onadd(pending, color);
		pending = null;
		text = '';
		input?.focus();
	}
</script>

<div class="editor">
	{#if pending}
		<p class="ask">Pick a colour for “{pending}”</p>
		<div class="swatches" role="radiogroup" aria-label="Tag colour">
			{#each palette as c (c)}
				<button role="radio" aria-checked="false" aria-label="Colour {c}" style="--c: {c}" onclick={() => pick(c)}></button>
			{/each}
			<button class="cancel" onclick={() => (pending = null)}>Cancel</button>
		</div>
	{:else}
		<form
			onsubmit={(e) => {
				e.preventDefault();
				submit();
			}}
		>
			<input bind:this={input} bind:value={text} placeholder="Add a tag" maxlength="32" aria-label="New tag" spellcheck="false" />
		</form>
		{#if suggestions.length}
			<div class="suggest">
				{#each suggestions as s (s)}
					<button style="--tag: {app.meta.tag_colors[s] ?? 'var(--muted)'}" onclick={() => submit(s)}><span class="dot"></span>{s}</button>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<style>
	.editor {
		display: grid;
		gap: 8px;
	}
	input {
		width: 100%;
		height: 32px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		outline: none;
		user-select: text;
		box-sizing: border-box;
	}
	input:focus {
		box-shadow: inset 0 0 0 1px var(--muted);
	}
	.ask {
		margin: 0;
		font-size: 13px;
		color: var(--muted);
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.swatches button[role='radio'] {
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 50%;
		background: var(--c);
		cursor: pointer;
	}
	.swatches button[role='radio']:hover {
		box-shadow: 0 0 0 2px var(--ground), 0 0 0 4px var(--c);
	}
	.cancel {
		margin-left: auto;
		border: 0;
		background: none;
		color: var(--muted);
		font-size: 13px;
		cursor: pointer;
	}
	.suggest {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.suggest button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 24px;
		padding: 0 8px;
		border: 0;
		border-radius: 4px;
		background: var(--sunken);
		font-size: 12px;
		cursor: pointer;
	}
	.suggest button:hover {
		background: var(--line);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--tag);
	}
</style>

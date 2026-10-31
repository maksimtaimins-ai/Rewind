<!-- Theme grid. Each swatch is a tiny Rewind window drawn in that theme's colors. -->
<script lang="ts">
	import { themes, findTheme, SYSTEM, type Palette } from '#lib/themes.ts';

	let { value, onchange }: { value: string; onchange: (id: string) => void } = $props();

	const choices = [{ id: SYSTEM, name: 'Follow system' }, ...themes.map((t) => ({ id: t.id, name: t.name }))];

	const vars = (p: Palette) =>
		`--g:${p.ground};--l:${p.line};--r:${p.raised};--s:${p.sunken};--t:${p.text};--m:${p.faint};--a:${p.accent};--a2:${p.accent2 ?? p.accent};--sig:${p.signal}`;
</script>

{#snippet mini(p: Palette)}
	<span class="win" style={vars(p)}>
		<span class="rail"></span>
		<span class="body">
			<span class="head"><span class="dot"></span><span class="title"></span></span>
			<span class="tape"><span class="fill"></span><span class="mark"></span></span>
			<span class="btn"></span>
		</span>
	</span>
{/snippet}

<div class="grid" role="radiogroup" aria-label="Theme">
	{#each choices as c (c.id)}
		<button role="radio" aria-checked={c.id === value} class:on={c.id === value} onclick={() => onchange(c.id)}>
			{#if c.id === SYSTEM}
				<span class="split">
					{@render mini(findTheme('light').palette)}
					<span class="half">{@render mini(findTheme('dark').palette)}</span>
				</span>
			{:else}
				{@render mini(findTheme(c.id).palette)}
			{/if}
			<span class="name">{c.name}</span>
		</button>
	{/each}
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
		gap: 8px;
	}
	button {
		display: grid;
		gap: 6px;
		padding: 6px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--line);
		text-align: left;
		cursor: pointer;
	}
	button:hover {
		box-shadow: inset 0 0 0 1px var(--line-strong);
	}
	button.on {
		box-shadow: inset 0 0 0 2px var(--accent);
	}
	.name {
		font-size: 13px;
		color: var(--muted);
		padding: 0 2px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.on .name {
		color: var(--text);
	}

	.split {
		position: relative;
		display: block;
	}
	.half {
		position: absolute;
		inset: 0;
		clip-path: polygon(100% 0, 100% 100%, 0 100%);
	}

	/* The miniature window: rail, title with recording dot, tape, primary button. */
	.win {
		display: flex;
		aspect-ratio: 16 / 9;
		border-radius: 3px;
		overflow: hidden;
		background: var(--g);
		box-shadow: inset 0 0 0 1px var(--l);
	}
	.rail {
		width: 16%;
		background: var(--s);
	}
	.body {
		flex: 1;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 12%;
		padding: 0 10%;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6%;
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--sig);
	}
	.title {
		width: 46%;
		height: 4px;
		border-radius: 2px;
		background: var(--t);
	}
	.tape {
		position: relative;
		height: 8px;
		border-radius: 2px;
		background: var(--r);
	}
	.fill {
		position: absolute;
		inset: 0 30% 0 0;
		border-radius: 2px;
		background: var(--m);
		opacity: 0.6;
	}
	.mark {
		position: absolute;
		left: 48%;
		top: -2px;
		bottom: -2px;
		width: 2px;
		background: var(--a2);
	}
	.btn {
		width: 34%;
		height: 6px;
		border-radius: 2px;
		background: var(--a);
	}
</style>

<!-- Result of the settings benchmark: verdict first, then the numbers behind it. -->
<script lang="ts">
	import type { BenchReport } from '#lib/api.ts';

	let { report: r }: { report: BenchReport } = $props();

	const pct = (v: number | null) => (v == null ? 'n/a' : `${v.toFixed(0)}%`);
	const rows = $derived<[string, string, string?][]>([
		['Encoder', r.encoder],
		['Resolution', `${r.width}×${r.height}`],
		['Frames captured', `${r.capture_fps.toFixed(1)} fps`, `cap ${r.target_fps}`],
		['Frames encoded', `${r.encoded_fps.toFixed(1)} fps`],
		['Encoder can sustain', r.encoder_max_fps > 0 ? `${Math.round(r.encoder_max_fps)} fps` : 'n/a', `needs ${r.target_fps}`],
		['Encoder load', `${Math.round(r.encoder_load * 100)}%`],
		['Dropped frames', String(r.dropped)],
		['Bitrate', `${(r.bitrate_kbps / 1000).toFixed(1)} Mb/s`],
		['CPU', `${r.cpu_pct.toFixed(0)}%`, `Rewind ${r.rewind_cpu_pct.toFixed(1)}%`],
		['GPU', pct(r.gpu_pct), `video encode ${pct(r.gpu_encode_pct)}`],
		['RAM', `${Math.round(r.ram_mb)} MB`, `buffer ${Math.round(r.buffer_mb)} MB`]
	]);
</script>

<div class="report {r.verdict.grade}" role="status">
	<p class="headline">{r.verdict.headline}</p>
	{#each r.verdict.detail as d, i (i)}
		<p class="detail">{d}</p>
	{/each}
	<table>
		<tbody>
			{#each rows as [k, v, note] (k)}
				<tr>
					<th scope="row">{k}</th>
					<td class="num">{v}</td>
					<td class="note">{note ?? ''}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<p class="foot">Measured over <span class="num">{r.secs.toFixed(0)}s</span> of the live buffer at your current settings.</p>
</div>

<style>
	.report {
		margin: 4px 0 16px;
		padding: 12px 16px;
		border-radius: 6px;
		background: var(--raised);
		box-shadow: inset 3px 0 0 var(--text);
	}
	.report.warn {
		box-shadow: inset 3px 0 0 #f5a524;
	}
	.report.fail {
		box-shadow: inset 3px 0 0 var(--danger);
	}
	.headline {
		margin: 0 0 4px;
		font-weight: 600;
	}
	.detail {
		margin: 0 0 4px;
		color: var(--muted);
		font-size: 13px;
		max-width: 64ch;
	}
	table {
		width: 100%;
		margin-top: 8px;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		text-align: left;
		font-weight: 400;
		color: var(--muted);
		padding: 3px 12px 3px 0;
		width: 40%;
	}
	td {
		padding: 3px 12px 3px 0;
		white-space: nowrap;
	}
	.note {
		color: var(--faint);
		width: 100%;
	}
	.foot {
		margin: 8px 0 0;
		color: var(--faint);
		font-size: 12px;
	}
</style>

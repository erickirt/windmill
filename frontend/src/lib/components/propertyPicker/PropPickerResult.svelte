<script lang="ts">
	import ObjectViewer from './ObjectViewer.svelte'

	export let allowCopy = false
	export let result: any
	export let extraResults: any = undefined
	export let flow_input: any = undefined
</script>

<div class="w-full px-2">
	<span class="font-normal text-sm text-secondary">Result</span>
	<div class="overflow-y-auto mb-2 w-full">
		<ObjectViewer {allowCopy} json={{ result, ...(extraResults ? extraResults : {}) }} on:select />
	</div>
	{#if flow_input}
		<span class="font-normal text-sm text-secondary">Flow Input</span>
		<div class="overflow-y-auto w-full">
			<ObjectViewer {allowCopy} json={flow_input} prefix="flow_input" on:select />
		</div>
	{/if}
</div>

<script lang="ts">
	import { tick } from 'svelte'
	import PostgresTriggerEditorInner from './PostgresTriggerEditorInner.svelte'

	let { onUpdate } = $props()

	let open = $state(false)
	export async function openEdit(ePath: string, isFlow: boolean) {
		open = true
		await tick()
		drawer?.openEdit(ePath, isFlow)
	}

	export async function openNew(
		is_flow: boolean,
		initial_script_path?: string,
		defaultValues?: Record<string, any>
	) {
		open = true
		await tick()
		drawer?.openNew(is_flow, initial_script_path, defaultValues)
	}

	let drawer: PostgresTriggerEditorInner | undefined = $state()
</script>

{#if open}
	<PostgresTriggerEditorInner {onUpdate} bind:this={drawer} />
{/if}

<script lang="ts">
	import type { AppViewerContext } from '$lib/components/apps/types'
	import { getContext } from 'svelte'
	import { connectInput } from '../../appUtils'
	import ComponentOutputViewer from '../ComponentOutputViewer.svelte'
	import OutputHeader from './OutputHeader.svelte'

	const { connectingInput } = getContext<AppViewerContext>('AppViewerContext')

	interface Props {
		id: string;
		name: string;
		first?: boolean;
	}

	let { id, name, first = false }: Props = $props();
</script>

<OutputHeader  renamable={false} selectable={true} {id} {name} color="blue" {first}>
	{#snippet children({ render })}
		<ComponentOutputViewer
			{render}
			componentId={id}
			on:select={({ detail }) => {
				$connectingInput = connectInput($connectingInput, id, detail)
			}}
		/>
	{/snippet}
</OutputHeader>

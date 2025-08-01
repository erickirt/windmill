<script lang="ts">
	import { Button } from '$lib/components/common'
	import { getContext } from 'svelte'
	import { initConfig, initOutput } from '../../editor/appUtils'
	import { components } from '../../editor/component'
	import type { AppInput } from '../../inputType'
	import type { AppViewerContext, ComponentCustomCSS, RichConfigurations } from '../../types'
	import { initCss } from '../../utils'
	import AlignWrapper from '../helpers/AlignWrapper.svelte'
	import ResolveConfig from '../helpers/ResolveConfig.svelte'
	import type RunnableComponent from '../helpers/RunnableComponent.svelte'
	import RunnableWrapper from '../helpers/RunnableWrapper.svelte'
	import { twMerge } from 'tailwind-merge'
	import ResolveStyle from '../helpers/ResolveStyle.svelte'
	import { User } from 'lucide-svelte'

	interface Props {
		id: string
		componentInput: AppInput | undefined
		configuration: RichConfigurations
		recomputeIds?: string[] | undefined
		extraQueryParams?: Record<string, any>
		horizontalAlignment?: 'left' | 'center' | 'right' | undefined
		customCss?: ComponentCustomCSS<'formcomponent'> | undefined
		render: boolean
		errorHandledByComponent?: boolean | undefined
	}

	let {
		id,
		componentInput,
		configuration,
		recomputeIds = undefined,
		extraQueryParams = {},
		horizontalAlignment = undefined,
		customCss = undefined,
		render,
		errorHandledByComponent = $bindable(false)
	}: Props = $props()

	export const staticOutputs: string[] = ['loading', 'result', 'jobId']

	const { app, worldStore, stateId, componentControl } =
		getContext<AppViewerContext>('AppViewerContext')

	const outputs = initOutput($worldStore, id, {
		result: undefined,
		loading: false,
		jobId: undefined
	})

	const resolvedConfig = $state(
		initConfig(components['formcomponent'].initialData.configuration, configuration)
	)

	$componentControl[id] = {
		setValue(nvalue: string) {
			wrapper?.setArgs(nvalue)
		},
		invalidate(key: string, error: string) {
			runnableComponent?.invalidate(key, error)
		},
		validateAll() {
			runnableComponent?.validateAll()
		},
		validate(key: string) {
			runnableComponent?.validate(key)
		}
	}

	let runnableComponent: RunnableComponent | undefined = $state()
	let loading = $state(false)

	let css = $state(initCss($app.css?.formcomponent, customCss))

	let wrapper: RunnableWrapper | undefined = $state()
	$effect(() => {
		errorHandledByComponent = resolvedConfig?.onError?.selected !== 'errorOverlay'
	})
	let noInputs = $derived(
		$stateId != undefined &&
			(componentInput?.type != 'runnable' || Object.keys(componentInput?.fields ?? {}).length == 0)
	)
</script>

{#each Object.keys(components['formcomponent'].initialData.configuration) as key (key)}
	<ResolveConfig
		{id}
		{key}
		bind:resolvedConfig={resolvedConfig[key]}
		configuration={configuration[key]}
	/>
{/each}

{#each Object.keys(css ?? {}) as key (key)}
	<ResolveStyle
		{id}
		{customCss}
		{key}
		bind:css={css[key]}
		componentStyle={$app.css?.formcomponent}
	/>
{/each}
<RunnableWrapper
	{recomputeIds}
	{render}
	bind:this={wrapper}
	bind:runnableComponent
	bind:loading
	{componentInput}
	{id}
	doOnSuccess={resolvedConfig.onSuccess}
	doOnError={resolvedConfig.onError}
	{errorHandledByComponent}
	{extraQueryParams}
	autoRefresh={false}
	forceSchemaDisplay={true}
	runnableClass={css?.container?.class}
	runnableStyle={css?.container?.style}
	{outputs}
>
	<AlignWrapper {horizontalAlignment}>
		<div
			class={twMerge('flex flex-col gap-2 px-4 w-full', css?.container?.class, 'wm-submit')}
			style={css?.container?.style ?? ''}
		>
			<div>
				{#if noInputs}
					<div class="text-secondary italic text-sm my-4">
						Run forms are associated with a runnable that has user inputs.
						<br />
						Once a script or flow is chosen, set some <strong>Runnable Inputs</strong> to
						<strong>
							User Input
							<User size={20} class="rounded-sm bg-gray-200 p-1 ml-0.5" />
						</strong>
					</div>
				{/if}
			</div>
			<div class="flex justify-end my-1">
				{#if !noInputs}
					<Button
						{loading}
						btnClasses={twMerge(css?.button?.class, 'wm-submit-button')}
						style={css?.button?.style ?? ''}
						on:pointerdown={(e) => {
							e?.stopPropagation()
						}}
						on:click={() => {
							runnableComponent?.runComponent()
						}}
						size={resolvedConfig.size}
						color={resolvedConfig.color}
					>
						{resolvedConfig.label}
					</Button>
				{/if}
			</div>
		</div>
	</AlignWrapper>
</RunnableWrapper>

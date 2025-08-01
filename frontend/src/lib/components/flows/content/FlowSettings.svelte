<script lang="ts">
	import { run, createBubbler } from 'svelte/legacy'

	const bubble = createBubbler()
	import Path from '$lib/components/Path.svelte'
	import FlowCard from '../common/FlowCard.svelte'
	import Toggle from '$lib/components/Toggle.svelte'
	import { Alert, Button, SecondsInput } from '$lib/components/common'
	import { getContext } from 'svelte'
	import type { FlowEditorContext } from '../types'
	import { enterpriseLicense, userStore, workspaceStore } from '$lib/stores'
	import { isCloudHosted } from '$lib/cloud'
	import Tooltip from '$lib/components/Tooltip.svelte'
	import SimpleEditor from '$lib/components/SimpleEditor.svelte'
	import { schemaToObject } from '$lib/schema'
	import type { Schema } from '$lib/common'
	import Section from '$lib/components/Section.svelte'
	import Label from '$lib/components/Label.svelte'
	import ErrorHandlerToggleButtonV2 from '$lib/components/details/ErrorHandlerToggleButtonV2.svelte'
	import WorkerTagPicker from '$lib/components/WorkerTagPicker.svelte'
	import MetadataGen from '$lib/components/copilot/MetadataGen.svelte'
	import Badge from '$lib/components/Badge.svelte'
	import { AlertTriangle } from 'lucide-svelte'
	import AIFormSettings from '$lib/components/copilot/AIFormSettings.svelte'

	interface Props {
		noEditor: boolean
		enableAi?: boolean
	}

	let { noEditor, enableAi }: Props = $props()

	const { flowStore, initialPathStore, previewArgs, pathStore, customUi } =
		getContext<FlowEditorContext>('FlowEditorContext')

	function asSchema(x: any) {
		return x as Schema
	}
	let path: Path | undefined = $state(undefined)
	let dirtyPath = $state(false)

	let displayWorkerTagPicker = $state(false)
	run(() => {
		flowStore.val.tag ? (displayWorkerTagPicker = true) : null
	})

	let activeAdvancedOptions = $derived([
		{
			name: 'High Priority',
			active: flowStore.val.value.priority !== undefined && flowStore.val.value.priority > 0
		},
		{ name: 'Error Handler Muted', active: Boolean(flowStore.val.ws_error_handler_muted) },
		{ name: 'Invisible to Others', active: Boolean(flowStore.val.visible_to_runner_only) },
		{ name: 'Shared Directory', active: Boolean(flowStore.val.value.same_worker) },
		{ name: 'Cache Results', active: Boolean(flowStore.val.value.cache_ttl) },
		{ name: 'Early Stop', active: Boolean(flowStore.val.value.skip_expr) },
		{ name: 'Early Return', active: Boolean(flowStore.val.value.early_return) },
		{ name: 'Dedicated Worker', active: Boolean(flowStore.val.dedicated_worker) },
		{ name: 'Concurrent Limit', active: Boolean(flowStore.val.value.concurrent_limit) },
		{ name: 'Run on Behalf of Last Editor', active: Boolean(flowStore.val.on_behalf_of_email) },
		{ name: 'Worker Tag', active: displayWorkerTagPicker }
	])

	let numberOfAdvancedOptionsOn = $derived(
		activeAdvancedOptions.filter((option) => option.active).length
	)
	let activeAdvancedOptionNames = $derived(
		activeAdvancedOptions.filter((option) => option.active).map((option) => option.name)
	)
</script>

<div class="h-full overflow-y-auto flex flex-col">
	<FlowCard {noEditor} title="Settings">
		<div class="grow min-h-0 p-4 h-full flex flex-col gap-8">
			<!-- Metadata Section -->
			<div class="gap-8 flex flex-col">
				<Label label="Summary">
					<MetadataGen
						bind:content={flowStore.val.summary}
						promptConfigName="flowSummary"
						flow={flowStore.val.value}
						on:change={() => {
							if ($initialPathStore == '' && flowStore.val.summary?.length > 0 && !dirtyPath) {
								path?.setName(
									flowStore.val.summary
										.toLowerCase()
										.replace(/[^a-z0-9_]/g, '_')
										.replace(/-+/g, '_')
										.replace(/^-|-$/g, '')
								)
							}
						}}
						elementProps={{
							type: 'text',
							id: 'flow-summary',
							placeholder: 'Short summary to be displayed when listed'
						}}
					/>
				</Label>

				{#if !noEditor}
					<Label label="Path">
						<Path
							autofocus={false}
							bind:this={path}
							bind:dirty={dirtyPath}
							bind:path={$pathStore}
							initialPath={$initialPathStore}
							namePlaceholder="flow"
							kind="flow"
						/>
					</Label>
				{/if}

				<Label label="Description">
					<MetadataGen
						bind:content={flowStore.val.description}
						promptConfigName="flowDescription"
						flow={flowStore.val.value}
						class="w-full"
						elementType="textarea"
						elementProps={{
							id: 'inp',
							placeholder: 'What this flow does and how to use it'
						}}
					/>
				</Label>

				{#if flowStore.val.schema && enableAi}
					<AIFormSettings
						bind:prompt={flowStore.val.schema.prompt_for_ai as string | undefined}
						type="flow"
					/>
				{/if}
			</div>

			<!-- Deployable Section -->
			<Section
				label="Advanced"
				collapsable={true}
				small={true}
				class="h-full grow  min-h-0 flex flex-col gap-4"
			>
				<!-- Worker Group Section -->
				{#if customUi?.settingsTabs?.workerGroup != false}
					<div>
						<Toggle
							textClass="font-normal text-sm"
							color="nord"
							size="xs"
							checked={displayWorkerTagPicker}
							on:change={() => {
								displayWorkerTagPicker = !displayWorkerTagPicker
								if (!displayWorkerTagPicker) {
									flowStore.val.tag = undefined
								}
							}}
							options={{
								right: 'Worker group tag (queue)',
								rightTooltip:
									"When a worker group tag is defined at the flow level, any steps inside the flow will run on any worker group that listen to that tag, regardless of the steps tag. If no worker group tags is defined, the flow controls will be executed with the default tag 'flow' and the steps will be executed with their respective tag",
								rightDocumentationLink: 'https://www.windmill.dev/docs/core_concepts/worker_groups'
							}}
							class="py-1"
						/>

						{#if displayWorkerTagPicker}
							<WorkerTagPicker bind:tag={flowStore.val.tag} popupPlacement="top-end" />
						{/if}
					</div>

					<!-- <Tooltip
								>In this mode, every scripts of this flow is run on the workers dedicated to this flow
								that keep the scripts "hot" so that there is not cold start cost incurred. Steps can run
								at >1500 rps in this mode.</Tooltip
							> -->
				{/if}

				<!-- Metadata Advanced Section -->
				{#snippet badge()}
					{#if numberOfAdvancedOptionsOn > 0}
						<div class="flex grow min-w-0 w-full flex-wrap gap-1 ps-2">
							{#each activeAdvancedOptionNames as optionName}
								<Badge twBgColor="bg-nord-950 dark:bg-nord-400" twTextColor="text-primary-inverse"
									>{optionName}</Badge
								>
							{/each}
						</div>
					{/if}
				{/snippet}

				<!-- Cache Section -->
				{#if customUi?.settingsTabs?.cache != false}
					<div>
						<Toggle
							textClass="font-normal text-sm"
							color="nord"
							size="xs"
							checked={Boolean(flowStore.val.value.cache_ttl)}
							on:change={() => {
								if (flowStore.val.value.cache_ttl && flowStore.val.value.cache_ttl != undefined) {
									flowStore.val.value.cache_ttl = undefined
								} else {
									flowStore.val.value.cache_ttl = 300
								}
							}}
							options={{
								right: 'Cache the results for each possible inputs',
								rightTooltip:
									'When enabled, the flow will cache the results of the flow for each possible set of inputs.',
								rightDocumentationLink: 'https://www.windmill.dev/docs/flows/cache#cache-flows'
							}}
							class="py-1"
						/>
						{#if flowStore.val.value.cache_ttl}
							<div class="flex gap-x-4 flex-col gap-1">
								<div class="text-sm text-secondary">How long to keep the cache valid</div>
								<div>
									{#if flowStore.val.value.cache_ttl}
										<SecondsInput bind:seconds={flowStore.val.value.cache_ttl} />
									{:else}
										<SecondsInput disabled />
									{/if}
								</div>
							</div>
						{/if}
					</div>
				{/if}

				<!-- Early Stop Section -->
				{#if customUi?.settingsTabs?.earlyStop != false}
					<div>
						<!-- documentationLink="https://www.windmill.dev/docs/flows/early_stop -->
						<Toggle
							textClass="font-normal text-sm"
							color="nord"
							size="xs"
							checked={Boolean(flowStore.val.value.skip_expr)}
							on:change={() => {
								if (Boolean(flowStore.val.value.skip_expr) && flowStore.val.value.skip_expr) {
									flowStore.val.value.skip_expr = undefined
								} else {
									flowStore.val.value.skip_expr = 'flow_input.foo == undefined'
								}
							}}
							options={{
								right: 'Early stop if condition met',
								rightTooltip:
									'If the inputs meet the predefined condition, the flow will not run.' +
									'to decide if the flow should stop early.',
								rightDocumentationLink:
									'https://www.windmill.dev/docs/flows/early_stop#early-stop-for-flow'
							}}
							class="py-1"
						/>
						{#if flowStore.val.value.skip_expr}
							<div
								class="w-full border flex flex-col {flowStore.val.value.skip_expr
									? ''
									: 'bg-surface-secondary'}"
							>
								<div class="border w-full">
									<SimpleEditor
										lang="javascript"
										bind:code={flowStore.val.value.skip_expr}
										class="small-editor"
										extraLib={`declare const flow_input = ${JSON.stringify(
											schemaToObject(asSchema(flowStore.val.schema), previewArgs.val)
										)};
									declare const WM_SCHEDULED_FOR: string;`}
									/>
									<div class="text-xs text-tertiary mt-2">
										You can use the variable `flow_input` to access the inputs of the flow. <br
										/>The variable `WM_SCHEDULED_FOR` contains the time the flow was scheduled for
										which you can use to stop early non fresh jobs:
										<pre>new Date().getTime() - new Date(WM_SCHEDULED_FOR).getTime() {'>'} X</pre>
									</div>
								</div>
							</div>
						{/if}
					</div>
				{/if}

				<!-- Early Return Section -->
				{#if customUi?.settingsTabs?.earlyReturn != false}
					<div>
						<Toggle
							textClass="font-normal text-sm"
							color="nord"
							size="xs"
							checked={Boolean(flowStore.val.value.early_return)}
							on:change={() => {
								if (Boolean(flowStore.val.value.early_return) && flowStore.val.value.early_return) {
									flowStore.val.value.early_return = undefined
								} else {
									flowStore.val.value.early_return = flowStore.val.value.modules?.[0]?.id ?? 'a'
								}
							}}
							options={{
								right: 'Early return for sync webhooks',
								rightTooltip:
									'If defined, sync endpoints will return early at the node defined here while the rest of the flow continue asynchronously.',
								rightDocumentationLink: 'https://www.windmill.dev/docs/flows/early_return'
							}}
							class="py-1"
						/>
						{#if flowStore.val.value.early_return}
							<div
								class="max-w-[120px] flex flex-col {flowStore.val.value.early_return
									? ''
									: 'bg-surface-secondary'}"
							>
								<select
									name="oauth_name"
									id="oauth_name"
									bind:value={flowStore.val.value.early_return}
									class="text-xs"
								>
									<option value={undefined}>Node's id</option>
									{#each flowStore.val.value?.modules?.map((x) => x.id) as name}
										<option value={name}>{name}</option>
									{/each}
								</select>
							</div>
						{/if}
					</div>
				{/if}

				<!-- Shared Directory Section -->
				{#if customUi?.settingsTabs?.sharedDiretory != false}
					<Toggle
						textClass="font-normal text-sm"
						color="nord"
						size="xs"
						bind:checked={flowStore.val.value.same_worker}
						options={{
							right: 'Same Worker + Shared directory on `./shared`',
							rightTooltip:
								'Steps will share a folder at `./shared` in which they can store heavier data and ' +
								'pass them to the next step. Beware that the `./shared` folder is not ' +
								'preserved across suspends and sleeps.',
							rightDocumentationLink:
								'https://www.windmill.dev/docs/core_concepts/persistent_storage/within_windmill#shared-directory'
						}}
						class="py-1"
					/>
				{/if}

				<!-- Visibility Section -->
				<Toggle
					textClass="font-normal text-sm"
					color="nord"
					size="xs"
					checked={Boolean(flowStore.val.visible_to_runner_only)}
					on:change={() => {
						if (flowStore.val.visible_to_runner_only) {
							flowStore.val.visible_to_runner_only = undefined
						} else {
							flowStore.val.visible_to_runner_only = true
						}
					}}
					options={{
						right: 'Make runs invisible to others',
						rightTooltip:
							'When this option is enabled, manual executions of this script are invisible to users other than the user running it, including the owner(s). This setting can be overridden when this script is run manually from the advanced menu.',
						rightDocumentationLink:
							'https://www.windmill.dev/docs/core_concepts/monitor_past_and_future_runs#invisible-runs'
					}}
					class="py-1"
				/>

				<!-- On behalf of last editor section -->
				<Toggle
					textClass="font-normal text-sm"
					color="nord"
					size="xs"
					checked={Boolean(flowStore.val.on_behalf_of_email)}
					on:change={() => {
						if (flowStore.val.on_behalf_of_email) {
							flowStore.val.on_behalf_of_email = undefined
						} else {
							flowStore.val.on_behalf_of_email = $userStore?.email
						}
					}}
					options={{
						right: 'Run on behalf of last editor',
						rightTooltip:
							'When this option is enabled, the flow will be run with the permissions of the last editor.'
					}}
					class="py-1"
				/>

				<!-- Error Handler Section -->
				<div class="flex flex-row items-center py-1">
					<ErrorHandlerToggleButtonV2
						color="nord"
						kind="flow"
						scriptOrFlowPath={$pathStore}
						bind:errorHandlerMuted={flowStore.val.ws_error_handler_muted}
					/>
					{#if !$enterpriseLicense}
						<span
							class="inline-flex text-xs items-center gap-1 !text-yellow-500 whitespace-nowrap ml-8"
						>
							<AlertTriangle size={16} />
							EE only <Tooltip>Enterprise Edition only feature</Tooltip>
						</span>
					{/if}
				</div>

				<!-- Concurrency Section -->
				{#if customUi?.settingsTabs?.concurrency != false}
					<div>
						<div class="flex flex-row items-center gap-2">
							<Toggle
								textClass="font-normal text-sm"
								color="nord"
								size="xs"
								disabled={!$enterpriseLicense}
								checked={Boolean(flowStore.val.value.concurrent_limit)}
								on:change={() => {
									if (flowStore.val.value.concurrent_limit) {
										flowStore.val.value.concurrent_limit = undefined
									} else {
										flowStore.val.value.concurrent_limit = 1
									}
								}}
								options={{
									right: 'Concurrency limits',
									rightTooltip: 'Allowed concurrency within a given timeframe',
									rightDocumentationLink: 'https://www.windmill.dev/docs/flows/concurrency_limit'
								}}
								class="py-1"
								eeOnly={true}
							/>
						</div>

						{#if flowStore.val.value.concurrent_limit}
							<div class="flex flex-col gap-4">
								<Label label="Max number of executions within the time window">
									<div class="flex flex-row gap-2 max-w-sm">
										<input
											disabled={!$enterpriseLicense}
											bind:value={flowStore.val.value.concurrent_limit}
											type="number"
										/>
										<Button
											size="sm"
											color="light"
											on:click={() => {
												flowStore.val.value.concurrent_limit = undefined
											}}
											variant="border">Remove Limits</Button
										>
									</div>
								</Label>
								<Label label="Time window in seconds">
									<SecondsInput
										disabled={!$enterpriseLicense}
										bind:seconds={flowStore.val.value.concurrency_time_window_s}
									/>
								</Label>
								<Label label="Custom concurrency key (optional)">
									{#snippet header()}
										<Tooltip>
											Concurrency keys are global, you can have them be workspace specific using the
											variable `$workspace`. You can also use an argument's value using
											`$args[name_of_arg]`</Tooltip
										>
									{/snippet}
									<!-- svelte-ignore a11y_autofocus -->
									<input
										type="text"
										autofocus
										disabled={!$enterpriseLicense}
										bind:value={flowStore.val.value.concurrency_key}
										placeholder={`$workspace/script/${$pathStore}-$args[foo]`}
									/>
								</Label>
							</div>
						{/if}
					</div>
				{/if}

				<!-- Priority Section -->
				<Toggle
					textClass="font-normal text-sm"
					color="nord"
					size="xs"
					disabled={!$enterpriseLicense || isCloudHosted()}
					checked={flowStore.val.value.priority !== undefined && flowStore.val.value.priority > 0}
					on:change={() => {
						if (flowStore.val.value.priority) {
							flowStore.val.value.priority = undefined
						} else {
							flowStore.val.value.priority = 100
						}
					}}
					options={{
						right: `Label as high priority`,
						rightTooltip: `All jobs scheduled by flows labeled as high priority take precedence over the other jobs in the jobs queue. Higher priority numbers are executed first. ${
							!$enterpriseLicense ? 'This is a feature only available on enterprise edition.' : ''
						}`,
						rightDocumentationLink: 'https://www.windmill.dev/docs/flows/priority'
					}}
					class="py-1 relative"
				>
					{#snippet right()}
						<input
							type="number"
							class="!w-16 text-xs ml-4 absolute left-52"
							disabled={flowStore.val.value.priority === undefined}
							bind:value={flowStore.val.value.priority}
							onfocus={bubble('focus')}
							onchange={() => {
								if (flowStore.val.value.priority && flowStore.val.value.priority > 100) {
									flowStore.val.value.priority = 100
								} else if (flowStore.val.value.priority && flowStore.val.value.priority < 0) {
									flowStore.val.value.priority = 0
								}
							}}
						/>
						{#if !$enterpriseLicense || isCloudHosted()}
							<span
								class="inline-flex absolute top-0 left-72 text-xs items-center gap-1 !text-yellow-500 whitespace-nowrap ml-8"
							>
								<AlertTriangle size={16} />
								EE only <Tooltip>Enterprise Edition only feature</Tooltip>
							</span>
						{/if}
					{/snippet}
				</Toggle>

				<div>
					<div class="flex flex-row items-center gap-2">
						<Toggle
							textClass="font-normal text-sm"
							color="nord"
							size="xs"
							disabled={!$enterpriseLicense || isCloudHosted()}
							checked={Boolean(flowStore.val.dedicated_worker)}
							on:change={() => {
								if (flowStore.val.dedicated_worker) {
									flowStore.val.dedicated_worker = undefined
								} else {
									flowStore.val.dedicated_worker = true
								}
							}}
							options={{
								right: 'Flow is run on dedicated workers',
								rightTooltip: 'When enabled, the flow will be executed on a dedicated worker.',
								rightDocumentationLink:
									'https://www.windmill.dev/docs/core_concepts/jobs#high-priority-jobs'
							}}
							class="py-1"
							eeOnly={true}
						/>
					</div>

					{#if flowStore.val.dedicated_worker}
						<div>
							<Alert type="info" title="Require dedicated workers">
								One worker in a worker group needs to be configured with dedicated worker set to: <pre
									>{$workspaceStore}:flow/{$pathStore}</pre
								>
							</Alert>
						</div>
					{/if}
				</div>
			</Section>
		</div>
	</FlowCard>
</div>

<script lang="ts">
	import { Button } from '$lib/components/common'
	import ToggleButton from '$lib/components/common/toggleButton-v2/ToggleButton.svelte'
	import ToggleButtonGroup from '$lib/components/common/toggleButton-v2/ToggleButtonGroup.svelte'
	import Tooltip from '$lib/components/Tooltip.svelte'
	import type { Relations } from '$lib/gen'
	import { Plus, Trash, X } from 'lucide-svelte'
	import MultiSelect from 'svelte-multiselect'
	import { invalidRelations } from './utils'
	import AddPropertyFormV2 from '$lib/components/schema/AddPropertyFormV2.svelte'
	import Label from '$lib/components/Label.svelte'
	import { emptyStringTrimmed, sendUserToast } from '$lib/utils'

	export let relations: Relations[] | undefined = undefined
	export let can_write: boolean = true
	export let disabled: boolean = false
	let selected: 'all' | 'specific' = relations && relations.length > 0 ? 'specific' : 'all'
	let cached: Relations[] | undefined = relations

	function addTable(name: string, index: number) {
		if (!relations || !Array.isArray(relations)) {
			relations = [
				{
					schema_name: 'public',
					table_to_track: []
				}
			]
		}
		relations[index].table_to_track = relations[index].table_to_track.concat({
			table_name: name,
			columns_name: []
		})
	}
</script>

<div class="flex flex-col gap-4 h-full">
	<div class="grow-0 flex flex-row gap-2 w-full items-center">
		<div class="grow-0">
			<ToggleButtonGroup
				on:selected={({ detail }) => {
					if (detail === 'all') {
						cached = relations
						relations = undefined
					} else {
						relations = cached ?? [
							{
								schema_name: 'public',
								table_to_track: []
							}
						]
					}
				}}
				bind:selected
				let:item
				{disabled}
			>
				<ToggleButton value="all" label="All Tables" {item} />
				<ToggleButton value="specific" label="Specific Tables" {item} />
			</ToggleButtonGroup>
		</div>
	</div>

	{#if relations && relations.length > 0}
		<div class="flex flex-col gap-4 grow min-h-0 overflow-y-auto">
			{#each relations as v, i}
				<div class="flex w-full gap-3 items-center">
					<div class="w-full flex flex-col gap-2 border py-2 px-4 rounded-md">
						<Label label="Schema Name" required class="w-full">
							<svelte:fragment slot="header">
								<Tooltip small>
									<p>
										Enter the name of the <strong>schema</strong> that contains the table(s) you
										want to track.
										<br />If no tables are added, all tables within the <strong>schema</strong> will
										be tracked for the selected transactions (insert, update, delete).
									</p>
								</Tooltip>
							</svelte:fragment>

							<input class="mt-1" type="text" bind:value={v.schema_name} {disabled} />
						</Label>
						<div class="flex flex-col w-full gap-4 items-center p-5">
							{#each v.table_to_track as table_to_track, j}
								<div
									class="relative rounded bg-surface-disabled p-2 flex w-full flex-col gap-4 group"
								>
									<Label label="Table Name" required>
										<svelte:fragment slot="header">
											<Tooltip small>Enter the name of the table you want to track.</Tooltip>
										</svelte:fragment>
										<input
											type="text"
											bind:value={table_to_track.table_name}
											class="!bg-surface mt-1"
											{disabled}
										/>
									</Label>
									<!-- svelte-ignore a11y-label-has-associated-control -->
									<Label label="Columns">
										<svelte:fragment slot="header">
											<Tooltip
												documentationLink="https://www.windmill.dev/docs/core_concepts/postgres_triggers#selecting-specific-columns"
												small
											>
												<p>
													Enter the names of the <strong>columns</strong> you want to track.
													<br />If no columns are specified, all columns in the table will be
													tracked for the selected transactions (insert, update, delete).
												</p>
												<p class="text-xs text-gray-500 mt-1">
													<strong class="font-semibold">Note:</strong>
													<br />- If your trigger contains <strong>UPDATE</strong> or
													<strong>DELETE</strong>
													transactions, the row filter WHERE clause must contain only columns covered
													by the <strong>replica identity</strong> (see REPLICA IDENTITY).
													<br />- If your trigger contains only <strong>INSERT</strong> transactions,
													the row filter WHERE clause can use any column.
												</p>
											</Tooltip>
										</svelte:fragment>
										<div class="mt-1">
											<MultiSelect
												options={table_to_track.columns_name ?? []}
												allowUserOptions="append"
												ulOptionsClass={'!bg-surface !text-sm'}
												ulSelectedClass="!text-sm"
												outerDivClass="!bg-surface !min-h-[38px] !border-[#d1d5db]"
												noMatchingOptionsMsg=""
												createOptionMsg={null}
												duplicates={false}
												selected={table_to_track.columns_name ?? []}
												placeholder="Select columns"
												--sms-options-margin="4px"
												{disabled}
												onchange={(e) => {
													const option = e.option?.toString()
													if (e.type === 'add') {
														option && table_to_track.columns_name?.push(option)
													} else if (e.type === 'remove') {
														table_to_track.columns_name = table_to_track.columns_name?.filter(
															(column) => column !== option
														)
													} else if (e.type === 'removeAll') {
														table_to_track.columns_name = []
													} else {
														console.error(
															`Priority tags multiselect - unknown event type: '${e.type}'`
														)
													}
												}}
											>
												<svelte:fragment slot="remove-icon">
													<div class="hover:text-primary p-0.5">
														<X size={12} />
													</div>
												</svelte:fragment>
											</MultiSelect>
										</div>
									</Label>
									<Label label="Where Clause">
										<svelte:fragment slot="header">
											<Tooltip
												documentationLink="https://www.windmill.dev/docs/core_concepts/postgres_triggers#filtering-rows-with-where-condition"
												small
											>
												<p class="text-sm">
													Use this field to define a row filter for the selected table. The <strong
														>WHERE</strong
													> clause allows only simple expressions and cannot include user-defined functions,
													operators, types, collations, system column references, or non-immutable built-in
													functions.
												</p>
												<p class="text-xs text-gray-500 mt-1">
													<strong class="font-semibold">Note:</strong>
													<br />- If your trigger contains <strong>UPDATE</strong> or
													<strong>DELETE</strong>
													transactions, the row filter WHERE clause must contain only columns covered
													by the <strong>replica identity</strong> (see REPLICA IDENTITY).
													<br />- If your trigger contains only <strong>INSERT</strong> transactions,
													the row filter WHERE clause can use any column.
												</p>
											</Tooltip>
										</svelte:fragment>
										<input
											type="text"
											bind:value={table_to_track.where_clause}
											class="!bg-surface mt-1"
											{disabled}
										/>
									</Label>
									<Button
										variant="border"
										wrapperClasses="absolute -top-3 -right-3"
										btnClasses="hidden group-hover:block hover:bg-red-500 hover:text-white p-2 rounded-full hover:border-red-500 transition-all duration-300"
										color="light"
										size="xs"
										on:click={() => {
											v.table_to_track = v.table_to_track.filter((_, index) => index !== j)
										}}
										iconOnly
										startIcon={{ icon: Trash }}
										{disabled}
									/>
								</div>
							{/each}
							<AddPropertyFormV2
								customName="Table"
								on:add={({ detail }) => {
									addTable(detail.name, i)
								}}
								{disabled}
							>
								<svelte:fragment slot="trigger">
									<Button
										wrapperClasses="w-full border border-dashed rounded-md"
										color="light"
										size="xs"
										{disabled}
										startIcon={{ icon: Plus }}
										nonCaptureEvent
									>
										Add table
									</Button>
								</svelte:fragment>
							</AddPropertyFormV2>
						</div>
					</div>
					<Button
						variant="border"
						color="light"
						size="xs"
						btnClasses="bg-surface-secondary hover:bg-red-500 hover:text-white p-2 rounded-full"
						aria-label="Clear"
						on:click={() => {
							if (relations) {
								relations = relations.filter((_, index) => index !== i)
							}
						}}
						{disabled}
					>
						<Trash size={14} />
					</Button>
				</div>
			{/each}
		</div>
	{/if}
	{#if selected === 'specific'}
		<div class="grow min-w-0 pr-10">
			<AddPropertyFormV2
				customName="Schema"
				on:add={({ detail }) => {
					if (relations == undefined || !Array.isArray(relations)) {
						relations = []
						relations = relations.concat({
							schema_name: 'public',
							table_to_track: []
						})
					} else if (emptyStringTrimmed(detail.name)) {
						sendUserToast('Schema name must not be empty', true)
					} else {
						const appendedRelations = relations.concat({
							schema_name: detail.name,
							table_to_track: []
						})
						if (
							invalidRelations(appendedRelations, {
								showError: true,
								trackSchemaTableError: false
							}) === ''
						) {
							relations = appendedRelations
						}
					}
				}}
				{disabled}
			>
				<svelte:fragment slot="trigger">
					<Button
						variant="border"
						color="light"
						size="xs"
						btnClasses="w-full"
						disabled={!can_write || disabled}
						startIcon={{ icon: Plus }}
						nonCaptureEvent
					>
						Add schema
					</Button>
				</svelte:fragment>
			</AddPropertyFormV2>
		</div>
	{/if}
</div>

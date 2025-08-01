<script lang="ts">
	import { InputService, type RunnableType } from '$lib/gen'
	import { workspaceStore } from '$lib/stores'
	import { onDestroy, untrack } from 'svelte'
	import InfiniteList from './InfiniteList.svelte'
	import JobSchemaPicker from './schema/JobSchemaPicker.svelte'

	interface Props {
		runnableId: string | undefined
		runnableType: RunnableType | undefined
		selected?: string | undefined
		showAuthor?: boolean
		placement?: 'bottom-start' | 'top-start' | 'bottom-end' | 'top-end'
		limitPayloadSize?: boolean
		searchArgs?: Record<string, any> | undefined
	}

	let {
		runnableId,
		runnableType,
		selected = undefined,
		showAuthor = false,
		placement = 'bottom-start',
		limitPayloadSize = false,
		searchArgs = undefined
	}: Props = $props()

	let infiniteList: InfiniteList | undefined = $state(undefined)
	let loadInputsPageFn: ((page: number, perPage: number) => Promise<any>) | undefined = undefined
	let viewerOpen = $state(false)
	let openStates: Record<string, boolean> = {} // Track open state for each item

	export async function refresh(clearCurrentRuns: boolean = false) {
		if (clearCurrentRuns) {
			infiniteList?.reset()
		}
		if (infiniteList) {
			await infiniteList.loadData('refresh')
		}
	}
	let cachedArgs: Record<string, any> = {}

	let timeout: NodeJS.Timeout | undefined = undefined
	function refreshInterval() {
		timeout && clearTimeout(timeout)
		timeout = setTimeout(async () => {
			await refresh()
			refreshInterval()
		}, 10000)
	}
	function initLoadInputs() {
		refreshInterval()
		loadInputsPageFn = async (page: number, perPage: number) => {
			const inputs = await InputService.getInputHistory({
				workspace: $workspaceStore!,
				runnableId,
				runnableType,
				page,
				perPage,
				includePreview: true,
				args: searchArgs ? JSON.stringify(searchArgs) : undefined
			})

			const inputsWithPayload = await Promise.all(
				inputs.map(async (input) => {
					if (cachedArgs[input.id]) {
						return {
							...input,
							payloadData: cachedArgs[input.id]
						}
					}
					const payloadData = await loadArgsFromHistory(input.id, undefined, false)
					if (payloadData === 'WINDMILL_TOO_BIG') {
						return {
							...input,
							payloadData: 'WINDMILL_TOO_BIG',
							getFullPayload: () => loadArgsFromHistory(input.id, undefined, true)
						}
					}
					cachedArgs[input.id] = payloadData
					return {
						...input,
						payloadData
					}
				})
			)
			return inputsWithPayload
		}
		infiniteList?.setLoader(loadInputsPageFn)
	}

	onDestroy(() => {
		timeout && clearTimeout(timeout)
	})

	async function loadArgsFromHistory(
		id: string | undefined,
		input: boolean | undefined,
		allowLarge: boolean
	): Promise<any> {
		if (!id) return
		const payloadData = await InputService.getArgsFromHistoryOrSavedInput({
			jobOrInputId: id,
			workspace: $workspaceStore!,
			input,
			allowLarge
		})
		return payloadData
	}

	function updateViewerOpenState(itemId: string, isOpen: boolean) {
		openStates[itemId] = isOpen
		viewerOpen = Object.values(openStates).some((state) => state)
	}

	$effect(() => {
		$workspaceStore && runnableId && runnableType && infiniteList && untrack(() => initLoadInputs())
	})
</script>

<InfiniteList bind:this={infiniteList} selectedItemId={selected} on:error on:select>
	{#snippet columns()}
		<colgroup>
			<col class="w-8" />
			<col class="w-16" />
			<col />
		</colgroup>
	{/snippet}
	{#snippet children({ item, hover })}
		<JobSchemaPicker
			job={item}
			hovering={hover}
			payloadData={item.payloadData}
			{showAuthor}
			{placement}
			{viewerOpen}
			on:openChange={({ detail }) => {
				updateViewerOpenState(item.id, detail)
			}}
			{limitPayloadSize}
		/>
	{/snippet}
	{#snippet empty()}
		<div class="text-center text-tertiary text-xs py-2">
			{runnableId ? 'No previous inputs' : 'Save draft to see previous runs'}
		</div>
	{/snippet}
</InfiniteList>

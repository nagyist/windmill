<script module lang="ts">
	export type DatatableContext = {
		size: 'xs' | 'sm' | 'md' | 'lg'
	}
</script>

<script lang="ts">
	import { createEventDispatcher, setContext, untrack } from 'svelte'
	import Button from '../common/button/Button.svelte'
	import { ArrowDownIcon, ArrowLeftIcon, ArrowRightIcon, Loader2 } from 'lucide-svelte'
	import { twMerge } from 'tailwind-merge'
	import List from '$lib/components/common/layout/List.svelte'

	let footerHeight: number = $state(0)
	let tableHeight: number = $state(0)
	const dispatch = createEventDispatcher()
	let tableContainer: HTMLDivElement | undefined = $state()
	interface Props {
		paginated?: boolean
		currentPage?: number
		showNext?: boolean
		showPrev?: boolean
		loadMore?: number
		shouldLoadMore?: boolean
		rounded?: boolean
		size?: 'xs' | 'sm' | 'md' | 'lg'
		perPage?: number | undefined
		shouldHidePagination?: boolean
		noBorder?: boolean
		rowCount?: number | undefined
		hasMore?: boolean
		contentHeight?: number
		tableFixed?: boolean
		infiniteScroll?: boolean | undefined
		neverShowLoader?: boolean
		loading?: boolean
		loadingMore?: boolean
		children?: import('svelte').Snippet
		emptyMessage?: import('svelte').Snippet
	}

	let {
		paginated = false,
		currentPage = $bindable(1),
		showNext = true,
		showPrev = true,
		loadMore = 0,
		shouldLoadMore = false,
		rounded = true,
		size = 'md',
		perPage = $bindable(undefined),
		shouldHidePagination = false,
		noBorder = false,
		rowCount = undefined,
		hasMore = true,
		contentHeight = $bindable(0),
		tableFixed = false,
		infiniteScroll = undefined,
		neverShowLoader = false,
		loading = false,
		loadingMore = false,
		children,
		emptyMessage
	}: Props = $props()
	setContext<DatatableContext>('datatable', {
		size
	})

	$effect(() => {
		contentHeight = tableHeight - footerHeight
	})

	function checkScrollStatus() {
		if (!infiniteScroll || loading || !tableContainer) return

		const hasScrollbar = tableContainer.scrollHeight > tableContainer.clientHeight
		if (!hasScrollbar && hasMore) {
			triggerLoadMore()
		}
	}

	function handleScroll() {
		if (!infiniteScroll || loading) {
			if (loading) {
				const checkAgain = () => {
					if (!loading) {
						handleScroll()
					}
				}
				setTimeout(checkAgain, 200)
			}
			return
		}

		if (!tableContainer) return
		const { scrollTop, scrollHeight, clientHeight } = tableContainer
		if (scrollHeight - (scrollTop + clientHeight) === 0) {
			triggerLoadMore()
		}
	}

	let lastLoadMoreDispatch: number | undefined = $state(undefined)
	function triggerLoadMore() {
		if (lastLoadMoreDispatch && Date.now() - lastLoadMoreDispatch < 1000) {
			return
		}
		dispatch('loadMore')
		lastLoadMoreDispatch = Date.now()
	}

	$effect(() => {
		if (tableContainer && hasMore && !loading) {
			untrack(() => checkScrollStatus())
		}
	})
</script>

<div
	class={twMerge(
		'h-full',
		rounded ? 'rounded-md overflow-hidden' : '',
		noBorder ? 'border-0' : 'border'
	)}
	bind:clientHeight={tableHeight}
>
	<List justify="between" gap="none" hFull={true}>
		<div class="w-full overflow-auto h-fit" bind:this={tableContainer} onscroll={handleScroll}>
			<table class={tableFixed ? 'table-fixed w-full' : 'min-w-full'}>
				{@render children?.()}
			</table>
			{@render emptyMessage?.()}
		</div>
		{#if paginated && !shouldHidePagination}
			<div
				class="w-full bg-surface border-t flex flex-row justify-between p-1 items-center gap-2 sticky bottom-0"
				bind:clientHeight={footerHeight}
			>
				<div>
					{#if rowCount}
						<span class="text-xs mx-2"> {rowCount} items</span>
					{/if}
				</div>

				<div class="flex flex-row gap-2 items-center">
					<span class="text-xs">
						Page: {currentPage}
						{perPage && rowCount ? `/ ${Math.ceil(rowCount / perPage)}` : ''}
					</span>

					{#if perPage !== undefined}
						<select class="!text-xs !w-16" bind:value={perPage}>
							<option value={25}>25</option>
							<option value={100}>100</option>
							<option value={1000}>1000</option>
						</select>
					{/if}
					{#if showPrev}
						<Button
							color="light"
							size="xs2"
							on:click={() => dispatch('previous')}
							disabled={currentPage === 1}
							startIcon={{ icon: ArrowLeftIcon }}
						>
							Previous
						</Button>
					{/if}
					{#if showNext}
						<Button
							color="light"
							size="xs2"
							on:click={() => dispatch('next')}
							endIcon={{ icon: ArrowRightIcon }}
							disabled={!hasMore}
						>
							Next
						</Button>
					{/if}
				</div>
			</div>
		{:else if shouldLoadMore}
			<div class="bg-surface border-t flex flex-row justify-center py-4 items-center gap-2">
				<Button
					color="light"
					size="xs2"
					on:click={() => triggerLoadMore()}
					endIcon={{ icon: ArrowDownIcon }}
				>
					Load {loadMore} more
				</Button>
			</div>
		{/if}
		{#if (loading || loadingMore) && !neverShowLoader}
			<div
				class="text-tertiary bg-surface border-t flex flex-row justify-center py-2 items-center gap-2"
			>
				<Loader2 class="animate-spin" size={14} />
				{#if loadingMore}
					<span class="text-xs">Loading more...</span>
				{/if}
			</div>
		{/if}
	</List>
</div>

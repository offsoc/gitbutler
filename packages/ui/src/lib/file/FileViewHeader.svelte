<script lang="ts">
	import Badge from '$lib/Badge.svelte';
	import Button from '$lib/Button.svelte';
	import Icon from '$lib/Icon.svelte';
	import FileName from '$lib/file/FileName.svelte';
	import FileStats from '$lib/file/FileStats.svelte';
	import type { FileStatus } from '$lib/file/types';

	interface Props {
		id?: string;
		filePath: string;
		fileStatus?: FileStatus;
		draggable?: boolean;
		linesAdded?: number;
		linesRemoved?: number;
		conflicted?: boolean;
		hasBorder?: boolean;
		oncontextmenu?: (e: MouseEvent) => void;
		oncloseclick?: () => void;
	}

	const {
		id,
		filePath,
		fileStatus,
		draggable = true,
		linesAdded = 0,
		linesRemoved = 0,
		conflicted = false,
		oncontextmenu,
		oncloseclick
	}: Props = $props();
</script>

<div
	role="presentation"
	{id}
	class="file-header"
	class:draggable
	{draggable}
	oncontextmenu={(e) => {
		if (oncontextmenu) {
			e.preventDefault();
			e.stopPropagation();
			oncontextmenu(e);
		}
	}}
>
	<div class="file-header__name">
		<div class="file-header__drag-handle">
			<Icon name="draggable-narrow" />
		</div>

		<FileName {filePath} textSize="13" />
	</div>

	<div class="file-header__statuses">
		<FileStats status={fileStatus} added={linesAdded} removed={linesRemoved} />
		{#if conflicted}
			<Badge size="icon" style="error">Has conflicts</Badge>
		{/if}

		{#if oncloseclick}
			<Button
				class="file-header__close-btn"
				kind="ghost"
				size="tag"
				icon="cross"
				onclick={oncloseclick}
			/>
		{/if}
	</div>
</div>

<style lang="postcss">
	.file-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px;
		width: 100%;
		background-color: var(--clr-bg-1);

		&.draggable {
			cursor: grab;

			&:hover {
				& .file-header__drag-handle {
					opacity: 1;
				}
			}
		}

		& :global(.file-header__close-btn) {
			margin-left: 8px;
		}
	}

	.file-header__statuses {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.file-header__name {
		display: flex;
		align-items: center;
		flex: 1;
	}

	.file-header__drag-handle {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 10px;
		margin-left: -8px;
		opacity: 0;
		color: var(--clr-text-3);
		transition:
			width var(--transition-fast),
			opacity var(--transition-fast);
	}
</style>

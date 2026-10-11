/*---------------------------------------------------------------------------------------------
 *  Copyright 2025 Glass Devtools, Inc. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------------*/

import React from 'react';
import { Plus, Lock, Mic } from 'lucide-react';
import { FeatureName } from '../../../../../../common/orbitSettingsTypes.js';
import { StagingSelectionItem } from '../../../../../../common/chatThreadServiceTypes.js';
import { ModelDropdown } from '../../../orbit-settings-tsx/ModelDropdown.js';
import { IconX } from '../icons/IconX.js';
import { ButtonStop } from '../buttons/ButtonStop.js';
import { ButtonSubmit } from '../buttons/ButtonSubmit.js';
import { ChatModeDropdown } from './ChatModeDropdown.js';
import { SelectedFiles } from '../files/SelectedFiles.js';

interface VoidChatAreaProps {
	// Required
	children: React.ReactNode; // This will be the input component

	// Form controls
	onSubmit: () => void;
	onAbort: () => void;
	isStreaming: boolean;
	isDisabled?: boolean;
	hasMessageToSubmit?: boolean;
	divRef?: React.RefObject<HTMLDivElement | null>;

	// UI customization
	className?: string;
	showModelDropdown?: boolean;
	showSelections?: boolean;
	showProspectiveSelections?: boolean;
	loadingIcon?: React.ReactNode;

	selections?: StagingSelectionItem[]
	setSelections?: (s: StagingSelectionItem[]) => void
	// selections?: any[];
	// onSelectionsChange?: (selections: any[]) => void;

	onClickAnywhere?: () => void;
	// Optional close button
	onClose?: () => void;
	// Optional image button in bottom row
	imageButton?: React.ReactNode;
	// Drag and drop handlers for images
	onDragEnter?: (e: React.DragEvent) => void;
	onDragOver?: (e: React.DragEvent) => void;
	onDragLeave?: (e: React.DragEvent) => void;
	onDrop?: (e: React.DragEvent) => void;
	isDragOver?: boolean;

	featureName: FeatureName;
}

export const VoidChatArea: React.FC<VoidChatAreaProps> = ({
	children,
	onSubmit,
	onAbort,
	onClose,
	onClickAnywhere,
	divRef,
	isStreaming = false,
	isDisabled = false,
	hasMessageToSubmit = false,
	className = '',
	showModelDropdown = true,
	showSelections = false,
	showProspectiveSelections = false,
	selections,
	setSelections,
	featureName,
	loadingIcon,
	imageButton,
	onDragEnter,
	onDragOver,
	onDragLeave,
	onDrop,
	isDragOver = false,
}) => {
	return (
		<div
			ref={divRef}
			className={`
				void-chat-area
				flex flex-col p-2 relative text-left shrink-0
				rounded-md
				bg-[var(--vscode-input-background)]
				text-[var(--vscode-input-foreground)]
				transition-all duration-200
				border ${isDragOver ? 'border-void-border-1 border-2 border-dashed bg-void-bg-2-alt/50 ring-2 ring-void-border-1/30' : 'border-[var(--void-sidebar-surface-border)]'} hover:border-[var(--void-sidebar-surface-border-hover)]
				max-h-[25vh] overflow-hidden
				${className}
			`}
			onClick={(e) => {
				onClickAnywhere?.()
			}}
			onDragEnter={onDragEnter}
			onDragOver={onDragOver}
			onDragLeave={onDragLeave}
			onDrop={onDrop}
		>
			{/* Scrollable content */}
			<div className="flex flex-col gap-2 min-h-0 overflow-y-auto pr-1 grow">
				{/* Selections section */}
				{showSelections && selections && setSelections && (
					<SelectedFiles
						type='staging'
						selections={selections}
						setSelections={setSelections}
						showProspectiveSelections={showProspectiveSelections}
					/>
				)}
				{/* Input section */}
				<div className="relative w-full">
					{children}
					{/* Close button (X) if onClose is provided */}
					{onClose && (
						<button
							type="button"
							className='absolute -top-1 -right-1 cursor-pointer z-10 p-0.5 hover:bg-white/10 rounded transition-colors'
							onClick={onClose}
							aria-label="Close"
						>
							<IconX
								size={12}
								className="stroke-[2] opacity-80 text-void-fg-3 hover:brightness-95"
							/>
						</button>
					)}
				</div>
			</div>
			{/* Bottom row - Cursor parity toolbar */}
			<div className='flex flex-row justify-between items-center gap-2 shrink-0 pt-2 flex-nowrap'>
				{/* Bottom-left: + Attachment Button & Model Selector Badge */}
				<div className="flex items-center gap-2 text-nowrap min-w-0 overflow-hidden">
					{/* + Attachment button */}
					<button
						type="button"
						className="w-6 h-6 rounded-full bg-white/5 hover:bg-white/10 text-zinc-300 hover:text-white flex items-center justify-center border border-white/10 transition-colors shrink-0 cursor-pointer"
						title="Add attachment"
						aria-label="Add attachment"
						onClick={(e) => {
							e.stopPropagation();
						}}
					>
						<Plus size={13} className="stroke-[2.2]" />
					</button>

					{featureName === 'Chat' && (
						<ChatModeDropdown
							className="
								flex items-center gap-1
								px-2 py-0.5
								rounded-full
								bg-[var(--void-pill-card-bg)]
								text-[var(--void-fg-2)]
								text-xs
								cursor-pointer select-none
								hover:bg-[var(--void-pill-card-bg-hover)]
								transition-colors
								min-w-0
								shrink
								overflow-hidden whitespace-nowrap text-ellipsis
							"
						/>
					)}

					{/* Model selector badge with Lock icon */}
					{showModelDropdown && (
						<div className="flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-[#1e1e1e] border border-[#2e2e2e] text-xs text-zinc-200">
							<Lock size={11} className="text-zinc-400 shrink-0" />
							<ModelDropdown
								featureName={featureName}
								className="w-[120px] sm:w-[155px] min-w-[90px] text-xs leading-5 bg-transparent border-0 px-0 shrink grow"
							/>
						</div>
					)}
				</div>

				{/* Bottom-right: Microphone dictation & Submit button */}
				<div className="flex items-center gap-1.5 ml-auto shrink-0">
					{imageButton}
					{/* Microphone icon button (subtle monoline icon matching Cursor) */}
					<button
						type="button"
						className="p-1 rounded-md text-zinc-400 hover:text-zinc-200 hover:bg-white/5 transition-colors active:scale-95 cursor-pointer shrink-0"
						title="Voice Dictation"
						aria-label="Voice Dictation"
						onClick={(e) => {
							e.stopPropagation();
						}}
					>
						<Mic size={15} className="stroke-[1.8]" />
					</button>

					{isStreaming && loadingIcon}
					{isStreaming ? (
						<ButtonStop onClick={onAbort} />
					) : (
						hasMessageToSubmit && (
							<ButtonSubmit
								onClick={onSubmit}
								disabled={isDisabled}
							/>
						)
					)}
				</div>
			</div>
		</div>
	);
};

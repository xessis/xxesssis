/*---------------------------------------------------------------------------------------------
 *  Copyright 2025 Glass Devtools, Inc. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------------*/

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { TextShimmer } from '../../../util/TextShimmer.js';
import { CollapsibleSection } from '../wrappers/CollapsibleSection.js';

interface ReasoningWrapperProps {
	isDoneReasoning: boolean;
	isStreaming: boolean;
	reasoningContentLength?: number;
	children: React.ReactNode;
}

const formatThoughtDuration = (totalSeconds: number): string => {
	const seconds = Math.max(1, Math.round(totalSeconds));
	if (seconds < 60) {
		return `${seconds}s`;
	}
	const minutes = Math.floor(seconds / 60);
	const remainder = seconds % 60;
	return remainder > 0 ? `${minutes}m ${remainder}s` : `${minutes}m`;
};

const estimateDurationSeconds = (contentLength: number): number => {
	// Rough estimate for committed messages loaded without live timing data
	return Math.max(1, Math.round(contentLength / 90));
};

export const ReasoningWrapper = ({
	isDoneReasoning,
	isStreaming,
	reasoningContentLength = 0,
	children,
}: ReasoningWrapperProps) => {
	const isActivelyThinking = isStreaming && !isDoneReasoning;
	const [isOpen, setIsOpen] = useState(isActivelyThinking);
	const [durationSeconds, setDurationSeconds] = useState<number | null>(null);
	const startTimeRef = useRef<number | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const wasActivelyThinkingRef = useRef(isActivelyThinking);

	// Track live duration while reasoning streams
	useEffect(() => {
		if (isActivelyThinking) {
			if (startTimeRef.current === null) {
				startTimeRef.current = Date.now();
			}
			setIsOpen(true);
			return;
		}

		if (startTimeRef.current !== null) {
			const elapsed = (Date.now() - startTimeRef.current) / 1000;
			setDurationSeconds(elapsed);
			startTimeRef.current = null;
		}
	}, [isActivelyThinking]);

	// Collapse to summary pill when reasoning finishes (keep header visible)
	useEffect(() => {
		if (wasActivelyThinkingRef.current && !isActivelyThinking) {
			setIsOpen(false);
		}
		wasActivelyThinkingRef.current = isActivelyThinking;
	}, [isActivelyThinking]);

	// Estimate duration for committed messages without live timing
	useEffect(() => {
		if (!isStreaming && isDoneReasoning && durationSeconds === null && reasoningContentLength > 0) {
			setDurationSeconds(estimateDurationSeconds(reasoningContentLength));
		}
	}, [isStreaming, isDoneReasoning, durationSeconds, reasoningContentLength]);

	// Auto-scroll while actively thinking and expanded
	useEffect(() => {
		if (!isOpen || !isActivelyThinking) return;

		const div = contentRef.current;
		if (!div) return;

		const rafId = requestAnimationFrame(() => {
			div.scrollTop = div.scrollHeight;
		});
		return () => cancelAnimationFrame(rafId);
	}, [reasoningContentLength, isOpen, isActivelyThinking]);

	const toggleOpen = useCallback(() => {
		setIsOpen(prev => !prev);
	}, []);

	const handleKeyDown = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			toggleOpen();
		}
	};

	const label = isActivelyThinking
		? 'Thinking...'
		: durationSeconds !== null
			? `Thought ${formatThoughtDuration(durationSeconds)}`
			: 'Thought';

	return (
		<div className="my-1.5">
			<button
				type="button"
				onClick={toggleOpen}
				onKeyDown={handleKeyDown}
				aria-expanded={isOpen}
				aria-controls="reasoning-content"
				className={`
					group inline-flex items-center gap-1.5
					px-2 py-0.5 rounded-md
					bg-void-bg-1/40 hover:bg-void-bg-1
					border border-void-border-3/40 hover:border-void-border-2/60
					cursor-pointer select-none
					text-void-fg-3 hover:text-void-fg-1 text-[12px]
					transition-all duration-150 ease-out shadow-xs
				`}
			>
				<ChevronRight
					size={12}
					strokeWidth={2.2}
					className={`
						flex-shrink-0 text-void-fg-3/70 group-hover:text-void-fg-1
						transition-transform duration-200 ease-out
						${isOpen ? 'rotate-90' : 'rotate-0'}
					`}
					aria-hidden="true"
				/>
				{isActivelyThinking ? (
					<span
						className="font-medium"
						style={{ color: 'var(--vscode-descriptionForeground)' }}
					>
						<TextShimmer duration={2.5} spread={2}>
							{label}
						</TextShimmer>
					</span>
				) : (
					<span className="font-medium truncate">{label}</span>
				)}
			</button>

			<CollapsibleSection isOpen={isOpen} contentClassName="mt-1.5 pl-4 border-l border-void-border-3/25">
				<div
					id="reasoning-content"
					role="region"
					aria-label="Reasoning content"
					ref={contentRef}
					className="
						max-h-[240px] overflow-y-auto
						text-void-fg-4 text-[13px] leading-relaxed
						void-custom-scrollable
					"
				>
					{children}
				</div>
			</CollapsibleSection>
		</div>
	);
};
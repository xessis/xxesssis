/*---------------------------------------------------------------------------------------------
 *  Copyright 2025 Glass Devtools, Inc. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------------*/

import React, { useState, useCallback } from 'react';
import { ThumbsUp, ThumbsDown, Copy, Check, GitFork } from 'lucide-react';
import { ChatMessage } from '../../../../../../common/chatThreadServiceTypes.js';
import { ChatMarkdownRender, ChatMessageLocation } from '../../../markdown/ChatMarkdownRender.js';
import { ProseWrapper } from '../wrappers/ProseWrapper.js';
import { SmallProseWrapper } from '../wrappers/SmallProseWrapper.js';
import { ReasoningWrapper } from './ReasoningWrapper.js';
import { useIsReadOnlyChat } from '../../contexts/ReadOnlyChatContext.js';
import { useAccessor } from '../../../util/services.js';

const EMPTY_MESSAGE_PLACEHOLDER = '(empty message)';

const isDisplayContentEmpty = (displayContent: string | undefined | null): boolean => {
	const trimmed = displayContent?.trim() ?? '';
	return trimmed.length === 0 || trimmed === EMPTY_MESSAGE_PLACEHOLDER;
};

export const AssistantMessageComponent = React.memo(({ chatMessage, isCheckpointGhost, isCommitted, messageIdx, threadId }: { chatMessage: ChatMessage & { role: 'assistant' }, isCheckpointGhost: boolean, messageIdx: number, isCommitted: boolean, threadId: string }) => {

	const isReadOnlyChat = useIsReadOnlyChat()
	const accessor = useAccessor()
	const [rating, setRating] = useState<'up' | 'down' | null>(null)
	const [copied, setCopied] = useState(false)

	const handleCopy = useCallback(() => {
		const text = chatMessage.displayContent || ''
		if (!text) return
		navigator.clipboard.writeText(text).then(() => {
			setCopied(true)
			setTimeout(() => setCopied(false), 1500)
		}).catch(() => {})
	}, [chatMessage.displayContent])

	const handleFork = useCallback(() => {
		try {
			const chatThreadsService = accessor.get('IChatThreadService')
			chatThreadsService.duplicateThread(threadId)
		} catch {}
	}, [accessor, threadId])

	const reasoningStr = chatMessage.reasoning?.trim() || null
	const hasReasoning = !!reasoningStr
	const hasDisplayContent = !isDisplayContentEmpty(chatMessage.displayContent)
	const isDoneReasoning = hasDisplayContent
	const chatMessageLocation: ChatMessageLocation | undefined = isReadOnlyChat ? undefined : {
		threadId,
		messageIdx: messageIdx,
	}
	const isLinkDetectionEnabled = !isReadOnlyChat

	if (!hasDisplayContent && !hasReasoning) return null

	return <div className={`w-full ${isCheckpointGhost ? 'opacity-50' : ''}`}>
		{/* reasoning token */}
		{hasReasoning &&
			<div className={`mb-2 last:mb-0 ${isCheckpointGhost ? 'opacity-50' : ''}`} data-orbit-chat-selectable data-orbit-quote-source='reasoning' data-orbit-thread-id={threadId} data-orbit-message-idx={messageIdx}>
				<ReasoningWrapper isDoneReasoning={isDoneReasoning} isStreaming={!isCommitted} reasoningContentLength={reasoningStr?.length ?? 0}>
					{/* Reasoning text reads as one calm, muted tone: bold, headings and
					    inline code inherit the body color instead of standing out bright/white. */}
					<div className="[&_strong]:!text-void-fg-4 [&_b]:!text-void-fg-4 [&_h1]:!text-void-fg-4 [&_h2]:!text-void-fg-4 [&_h3]:!text-void-fg-4 [&_h4]:!text-void-fg-4 [&_code]:!text-void-fg-4 [&_.orbit-inline-code]:!text-void-fg-4 [&_li]:!text-void-fg-4 [&_p]:!text-void-fg-4">
					<SmallProseWrapper>
						<ChatMarkdownRender
							string={reasoningStr}
							chatMessageLocation={chatMessageLocation}
							isApplyEnabled={false}
							isLinkDetectionEnabled={isLinkDetectionEnabled}
						/>
					</SmallProseWrapper>
					</div>
				</ReasoningWrapper>
			</div>
		}

		{/* assistant message */}
		{hasDisplayContent &&
			<div className={isCheckpointGhost ? 'opacity-50' : ''} data-orbit-chat-selectable data-orbit-quote-source='assistant' data-orbit-thread-id={threadId} data-orbit-message-idx={messageIdx}>
				<ProseWrapper>
					<ChatMarkdownRender
						string={chatMessage.displayContent || ''}
						chatMessageLocation={chatMessageLocation}
						isApplyEnabled={!isReadOnlyChat}
						isLinkDetectionEnabled={isLinkDetectionEnabled}
					/>
				</ProseWrapper>

				{/* Cursor-style footer action icons */}
				{!isReadOnlyChat && isCommitted && (
					<div className="flex items-center gap-1.5 mt-2.5 pt-1 text-void-fg-4 opacity-75 hover:opacity-100 transition-opacity">
						<button
							type="button"
							title="Good response"
							onClick={() => setRating(prev => prev === 'up' ? null : 'up')}
							className={`p-1 rounded-md hover:bg-void-bg-1/80 transition-colors ${rating === 'up' ? 'text-void-fg-1 bg-void-bg-1' : 'text-void-fg-4 hover:text-void-fg-2'}`}
						>
							<ThumbsUp size={13} strokeWidth={2} />
						</button>
						<button
							type="button"
							title="Bad response"
							onClick={() => setRating(prev => prev === 'down' ? null : 'down')}
							className={`p-1 rounded-md hover:bg-void-bg-1/80 transition-colors ${rating === 'down' ? 'text-void-fg-1 bg-void-bg-1' : 'text-void-fg-4'}`}
						>
							<ThumbsDown size={13} strokeWidth={2} />
						</button>
						<button
							type="button"
							title="Copy message"
							onClick={handleCopy}
							className="p-1 rounded-md hover:bg-void-bg-1/80 text-void-fg-4 hover:text-void-fg-2 transition-colors"
						>
							{copied ? <Check size={13} className="text-green-500" strokeWidth={2.5} /> : <Copy size={13} strokeWidth={2} />}
						</button>
						<button
							type="button"
							title="Fork chat"
							onClick={handleFork}
							className="p-1 rounded-md hover:bg-void-bg-1/80 text-void-fg-4 hover:text-void-fg-2 transition-colors"
						>
							<GitFork size={13} strokeWidth={2} />
						</button>
					</div>
				)}
			</div>
		}
	</div>

});

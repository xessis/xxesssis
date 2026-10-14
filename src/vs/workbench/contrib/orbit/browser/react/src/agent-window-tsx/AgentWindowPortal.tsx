/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import * as React from 'react';
import { createPortal } from 'react-dom';
import { ChatHistory } from '../chathistory-tsx/ChatHistory.js';
import { Sidebar } from '../sidebar-tsx/Sidebar.js';
import { VoidTooltip } from '../orbit-tooltip/orbitTooltip.js';
import { AgentWorkspace } from './AgentWorkspace.js';
import { AgentTopBar } from './AgentTopBar.js';
import { CustomizeApp } from '../customize-tsx/CustomizeApp.js';
import { Settings } from '../orbit-settings-tsx/Settings.js';
import ErrorBoundary from '../sidebar-tsx/ErrorBoundary.js';
import { useAccessor, useAgentWorkspaceState, useChatThreadsState } from '../util/services.js';

export type AgentWindowView = 'chat' | 'automations' | 'customize' | 'settings';

export const AgentWindowNavigationContext = React.createContext<{
	currentView: AgentWindowView;
	setCurrentView: (view: AgentWindowView) => void;
}>({
	currentView: 'chat',
	setCurrentView: () => {},
});

export const useAgentWindowNavigation = () => React.useContext(AgentWindowNavigationContext);

const AutomationsView = () => {
	const accessor = useAccessor();
	const commandService = accessor.get('ICommandService');

	return (
		<div className="w-full h-full flex flex-col overflow-auto bg-[#181818] text-zinc-200 p-8 select-none">
			<div className="max-w-[760px] mx-auto w-full">
				<h1 className="text-xl font-semibold text-white">Automations</h1>
				<p className="text-xs text-zinc-400 mt-1">
					Automate repetitive tasks with always-on agents and configure Cursor's built-in agents for your team.
				</p>

				<div className="mt-12 p-8 rounded-2xl bg-[#1e1e1e] border border-[#2e2e2e] text-center flex flex-col items-center">
					<p className="text-sm text-zinc-300 font-medium">
						Upgrade for Automations, unlimited completions, MAX Mode, and more
					</p>
					<button
						type="button"
						onClick={() => {
							try { commandService.executeCommand('workbench.action.openVoidSettings'); } catch {}
						}}
						className="mt-4 px-6 py-1.5 rounded-lg bg-[#3b82f6] hover:bg-[#2563eb] text-white text-xs font-medium transition-colors cursor-pointer"
					>
						Upgrade
					</button>
				</div>
			</div>
		</div>
	);
};

export type AgentWindowPortalTargets = {
	sidebarEl: HTMLElement;
	mainEl: HTMLElement;
	tooltipEl: HTMLElement;
	workspaceEl: HTMLElement;
};

const AgentWindowChat = () => {
	const { currentView } = useAgentWindowNavigation();
	const accessor = useAccessor();
	const workspaceState = useAgentWorkspaceState();
	const threadsState = useChatThreadsState();
	const chatThreadService = accessor.get('IChatThreadService');
	const threadId = threadsState.agentWindowThreadId;
	const selected = threadId ? threadsState.allThreads[threadId] : undefined;
	const selectedAgentWorkspaceId = selected?.agentWorkspaceId;

	React.useEffect(() => {
		// Always ensure scope on mount and whenever the active workspace or
		// selected thread changes. A persisted agentWindowThreadId from another
		// workspace (or an IDE-scoped thread with agentWorkspaceId undefined)
		// must not drive Self-hosted Runner submits with the wrong git context.
		if (!threadId || !selected || selectedAgentWorkspaceId !== workspaceState.activeWorkspaceId) {
			chatThreadService.ensureAgentWindowThread(workspaceState.activeWorkspaceId);
		}
	}, [chatThreadService, workspaceState.activeWorkspaceId, threadId, selected, selectedAgentWorkspaceId]);

	if (currentView === 'automations') {
		return <AutomationsView />;
	}
	if (currentView === 'customize') {
		return <div className="w-full h-full overflow-hidden bg-[#181818]"><CustomizeApp /></div>;
	}
	if (currentView === 'settings') {
		return <div className="w-full h-full overflow-hidden bg-[#181818]"><Settings /></div>;
	}

	const ready = !!threadId
		&& !!selected
		&& selectedAgentWorkspaceId === workspaceState.activeWorkspaceId;

	if (!ready) {
		// Lightweight placeholder (R4): avoids a blank flash while
		// ensureAgentWindowThread syncs the thread into the active workspace.
		// Keeps height/scroll so the surrounding portal layout doesn't jump.
		return (
			<div className="w-full h-full flex flex-col">
				<div className="flex-1 w-full flex items-center justify-center text-void-fg-3 text-sm select-none">
					Preparing agent…
				</div>
			</div>
		);
	}
	return (
		<div className="w-full h-full flex flex-col overflow-hidden">
			<div className="flex-1 min-h-0 overflow-hidden">
				<Sidebar key={threadId} className="" isAgentWindow />
			</div>
		</div>
	);
};

/**
 * Renders the agent-window UI via portals into auxiliary-window panes.
 * React roots stay in the main renderer document so event delegation and
 * `instanceof HTMLElement` checks keep working across windows.
 */
export const AgentWindowPortal = ({ sidebarEl, mainEl, tooltipEl, workspaceEl }: AgentWindowPortalTargets) => {
	const [currentView, setCurrentView] = React.useState<AgentWindowView>('chat');
	const navValue = React.useMemo(() => ({ currentView, setCurrentView }), [currentView]);

	return (
		<AgentWindowNavigationContext.Provider value={navValue}>
			{createPortal(<ErrorBoundary><ChatHistory isAgentWindow /></ErrorBoundary>, sidebarEl)}
			{createPortal(<ErrorBoundary><AgentWindowChat /></ErrorBoundary>, mainEl)}
			{createPortal(<ErrorBoundary><AgentWorkspace /></ErrorBoundary>, workspaceEl)}
			{createPortal(<ErrorBoundary><VoidTooltip /></ErrorBoundary>, tooltipEl)}
		</AgentWindowNavigationContext.Provider>
	);
};

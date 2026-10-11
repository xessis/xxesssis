/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import React, { useState, useRef, useEffect, useCallback } from 'react';
import {
	ExternalLink,
	MoreHorizontal,
	Minus,
	Square,
	X,
	PanelLeft,
	PanelRight,
	ArrowLeft,
	ArrowRight,
	Settings,
	BookOpen,
	FileText,
	RefreshCw,
	HelpCircle,
	Folder,
	Plus,
	SlidersHorizontal,
} from 'lucide-react';
import { useAccessor, useChatThreadsState } from '../util/services.js';
import { VOID_OPEN_ACCOUNT_SETTINGS_ACTION_ID } from '../../../actionIDs.js';

type MenuKind = 'file' | 'edit' | 'view' | 'help' | 'overflow' | null;

/**
 * Cursor isometric wireframe cube logo SVG
 */
const CursorCubeIcon = () => (
	<svg
		width="16"
		height="16"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		strokeWidth="1.8"
		strokeLinecap="round"
		strokeLinejoin="round"
		className="shrink-0 text-white opacity-90"
		aria-hidden="true"
	>
		<path d="M12 2L2 7l10 5 10-5-10-5z" />
		<path d="M2 17l10 5 10-5" />
		<path d="M2 12l10 5 10-5" />
	</svg>
);

export interface AgentTopBarProps {
	className?: string;
}

/**
 * Cursor-parity Top Bar:
 * - Top-left: Cursor cube logo, Native Menus (File, Edit, View, Help)
 * - Navigation: Sidebar toggle, History Back (←), Forward (→)
 * - Center: Active thread title or drag space
 * - Top-right: IDE ↗ button, Overflow (•••), layout toggle, window controls (—, ◻, ✕)
 */
export const AgentTopBar: React.FC<AgentTopBarProps> = ({ className = '' }) => {
	const accessor = useAccessor();
	const commandService = accessor.get('ICommandService');
	const agentWindowService = accessor.get('IAgentWindowService');
	const chatThreadService = accessor.get('IChatThreadService');
	const threadsState = useChatThreadsState();

	const [openMenu, setOpenMenu] = useState<MenuKind>(null);
	const barRef = useRef<HTMLDivElement>(null);

	const activeThreadId = threadsState.agentWindowThreadId ?? threadsState.currentThreadId;
	const activeThread = activeThreadId ? threadsState.allThreads[activeThreadId] : undefined;
	const firstUserMsg = activeThread?.messages.find(m => m.role === 'user');
	const threadTitle = (firstUserMsg && (firstUserMsg.displayContent || firstUserMsg.content)) || 'New Agent';

	// Close menus on outside click
	useEffect(() => {
		if (!openMenu) return;
		const handleClickOutside = (e: MouseEvent) => {
			if (barRef.current && !barRef.current.contains(e.target as Node)) {
				setOpenMenu(null);
			}
		};
		document.addEventListener('mousedown', handleClickOutside);
		return () => document.removeEventListener('mousedown', handleClickOutside);
	}, [openMenu]);

	const handleFocusIde = useCallback(() => {
		void agentWindowService.focusMainWindow();
	}, [agentWindowService]);

	const handleToggleSidebar = useCallback(() => {
		agentWindowService.toggleSidebar();
	}, [agentWindowService]);

	const handleToggleWorkspace = useCallback(() => {
		agentWindowService.toggleWorkspace();
	}, [agentWindowService]);

	const handleNewChat = useCallback(() => {
		setOpenMenu(null);
		chatThreadService.openNewThread({ forceNew: true });
	}, [chatThreadService]);

	const handleOpenCustomize = useCallback(() => {
		setOpenMenu(null);
		void commandService.executeCommand('workbench.action.openVoidCustomize');
	}, [commandService]);

	const handleOpenSettings = useCallback(() => {
		setOpenMenu(null);
		void commandService.executeCommand('workbench.action.openVoidSettings');
	}, [commandService]);

	const handleCloseWindow = useCallback(() => {
		if (typeof window !== 'undefined') {
			window.close();
		}
	}, []);

	return (
		<div
			ref={barRef}
			className={`cursor-top-bar select-none relative z-50 flex items-center justify-between px-2.5 h-9 bg-[#121212] border-b border-[#252525] text-zinc-300 text-xs ${className}`}
			style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
		>
			{/* Left section: App logo, Menus, Nav buttons */}
			<div
				className="flex items-center gap-1 min-w-0"
				style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
			>
				{/* Logo */}
				<div className="flex items-center pr-1.5" title="Orbit">
					<CursorCubeIcon />
				</div>

				{/* Menus */}
				<div className="relative">
					<button
						type="button"
						className={`cursor-topbar-menu-item ${openMenu === 'file' ? 'bg-white/10 text-white' : ''}`}
						onClick={() => setOpenMenu(openMenu === 'file' ? null : 'file')}
					>
						File
					</button>
					{openMenu === 'file' && (
						<div className="cursor-dropdown-menu">
							<button type="button" className="cursor-dropdown-item" onClick={handleNewChat}>
								<Plus size={13} />
								<span className="flex-1">New Chat</span>
								<span className="cursor-dropdown-shortcut">Ctrl+N</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('workbench.action.files.openFolder');
								}}
							>
								<Folder size={13} />
								<span className="flex-1">Open Folder…</span>
								<span className="cursor-dropdown-shortcut">Ctrl+O</span>
							</button>
							<div className="cursor-dropdown-divider" />
							<button type="button" className="cursor-dropdown-item" onClick={handleCloseWindow}>
								<X size={13} />
								<span className="flex-1">Close Window</span>
								<span className="cursor-dropdown-shortcut">Ctrl+W</span>
							</button>
						</div>
					)}
				</div>

				<div className="relative">
					<button
						type="button"
						className={`cursor-topbar-menu-item ${openMenu === 'edit' ? 'bg-white/10 text-white' : ''}`}
						onClick={() => setOpenMenu(openMenu === 'edit' ? null : 'edit')}
					>
						Edit
					</button>
					{openMenu === 'edit' && (
						<div className="cursor-dropdown-menu">
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('undo');
								}}
							>
								<span>Undo</span>
								<span className="cursor-dropdown-shortcut">Ctrl+Z</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('redo');
								}}
							>
								<span>Redo</span>
								<span className="cursor-dropdown-shortcut">Ctrl+Y</span>
							</button>
							<div className="cursor-dropdown-divider" />
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('editor.action.clipboardCutAction');
								}}
							>
								<span>Cut</span>
								<span className="cursor-dropdown-shortcut">Ctrl+X</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('editor.action.clipboardCopyAction');
								}}
							>
								<span>Copy</span>
								<span className="cursor-dropdown-shortcut">Ctrl+C</span>
							</button>
						</div>
					)}
				</div>

				<div className="relative">
					<button
						type="button"
						className={`cursor-topbar-menu-item ${openMenu === 'view' ? 'bg-white/10 text-white' : ''}`}
						onClick={() => setOpenMenu(openMenu === 'view' ? null : 'view')}
					>
						View
					</button>
					{openMenu === 'view' && (
						<div className="cursor-dropdown-menu">
							<button type="button" className="cursor-dropdown-item" onClick={() => { setOpenMenu(null); handleToggleSidebar(); }}>
								<PanelLeft size={13} />
								<span className="flex-1">Toggle Sidebar</span>
								<span className="cursor-dropdown-shortcut">Ctrl+B</span>
							</button>
							<button type="button" className="cursor-dropdown-item" onClick={() => { setOpenMenu(null); handleToggleWorkspace(); }}>
								<PanelRight size={13} />
								<span className="flex-1">Toggle Workspace</span>
								<span className="cursor-dropdown-shortcut">Ctrl+Shift+B</span>
							</button>
							<div className="cursor-dropdown-divider" />
							<button type="button" className="cursor-dropdown-item" onClick={handleOpenCustomize}>
								<SlidersHorizontal size={13} />
								<span className="flex-1">Customize Orbit…</span>
							</button>
						</div>
					)}
				</div>

				<div className="relative">
					<button
						type="button"
						className={`cursor-topbar-menu-item ${openMenu === 'help' ? 'bg-white/10 text-white' : ''}`}
						onClick={() => setOpenMenu(openMenu === 'help' ? null : 'help')}
					>
						Help
					</button>
					{openMenu === 'help' && (
						<div className="cursor-dropdown-menu">
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									if (typeof window !== 'undefined') {
										window.open('https://orbit-editor.com/docs', '_blank');
									}
								}}
							>
								<BookOpen size={13} />
								<span className="flex-1">Orbit Documentation</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('workbench.action.openGlobalKeybindings');
								}}
							>
								<FileText size={13} />
								<span className="flex-1">Keyboard Shortcuts</span>
							</button>
							<div className="cursor-dropdown-divider" />
							<button type="button" className="cursor-dropdown-item" onClick={handleOpenSettings}>
								<Settings size={13} />
								<span className="flex-1">Settings</span>
							</button>
						</div>
					)}
				</div>

				{/* Navigation buttons: Sidebar toggle, Back, Forward */}
				<div className="flex items-center gap-0.5 ml-2 pl-2 border-l border-[#2a2a2a]">
					<button
						type="button"
						className="cursor-topbar-btn"
						title="Toggle Chat History"
						aria-label="Toggle Chat History"
						onClick={handleToggleSidebar}
					>
						<PanelLeft size={14} />
					</button>
					<button
						type="button"
						className="cursor-topbar-btn"
						title="Back"
						aria-label="Back"
						onClick={() => {
							try { history.back(); } catch { /* ignore */ }
						}}
					>
						<ArrowLeft size={13} />
					</button>
					<button
						type="button"
						className="cursor-topbar-btn"
						title="Forward"
						aria-label="Forward"
						onClick={() => {
							try { history.forward(); } catch { /* ignore */ }
						}}
					>
						<ArrowRight size={13} />
					</button>
				</div>
			</div>

			{/* Center title / drag space */}
			<div className="flex-1 flex items-center justify-center min-w-0 px-4 truncate text-center text-[11px] text-zinc-400 font-normal">
				<span className="truncate opacity-80" title={threadTitle}>
					{threadTitle}
				</span>
			</div>

			{/* Right section: IDE ↗, Overflow, Layout toggle, Window controls */}
			<div
				className="flex items-center gap-1 shrink-0"
				style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
			>
				{/* IDE ↗ button */}
				<button
					type="button"
					className="cursor-ide-btn"
					title="Focus IDE Window"
					aria-label="Focus IDE Window"
					onClick={handleFocusIde}
				>
					<span>IDE</span>
					<ExternalLink size={12} strokeWidth={2} />
				</button>

				{/* Overflow menu */}
				<div className="relative">
					<button
						type="button"
						className={`cursor-topbar-btn ${openMenu === 'overflow' ? 'bg-white/10 text-white' : ''}`}
						title="More options"
						aria-label="More options"
						onClick={() => setOpenMenu(openMenu === 'overflow' ? null : 'overflow')}
					>
						<MoreHorizontal size={14} />
					</button>
					{openMenu === 'overflow' && (
						<div className="cursor-dropdown-menu right-0">
							<button type="button" className="cursor-dropdown-item" onClick={handleOpenSettings}>
								<Settings size={13} />
								<span className="flex-1">Settings</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('workbench.action.reloadWindow');
								}}
							>
								<RefreshCw size={13} />
								<span className="flex-1">Reload Window</span>
							</button>
							<button
								type="button"
								className="cursor-dropdown-item"
								onClick={() => {
									setOpenMenu(null);
									void commandService.executeCommand('workbench.action.toggleDevTools');
								}}
							>
								<HelpCircle size={13} />
								<span className="flex-1">Developer Tools</span>
							</button>
						</div>
					)}
				</div>

				{/* Layout toggle */}
				<button
					type="button"
					className="cursor-topbar-btn"
					title="Toggle Agent Workspace"
					aria-label="Toggle Agent Workspace"
					onClick={handleToggleWorkspace}
				>
					<PanelRight size={14} />
				</button>

				{/* Window controls (Windows/Linux) */}
				<div className="flex items-center gap-0.5 ml-1 pl-1 border-l border-[#2a2a2a]">
					<button
						type="button"
						className="cursor-window-ctrl"
						title="Minimize"
						aria-label="Minimize"
						onClick={() => {
							try {
								// Trigger window minimize
								(window as unknown as { electron?: { minimize?: () => void } }).electron?.minimize?.();
							} catch { /* ignore */ }
						}}
					>
						<Minus size={12} />
					</button>
					<button
						type="button"
						className="cursor-window-ctrl"
						title="Maximize"
						aria-label="Maximize"
						onClick={() => {
							try {
								// Trigger window maximize
								(window as unknown as { electron?: { maximize?: () => void } }).electron?.maximize?.();
							} catch { /* ignore */ }
						}}
					>
						<Square size={10} />
					</button>
					<button
						type="button"
						className="cursor-window-ctrl cursor-window-ctrl--close"
						title="Close"
						aria-label="Close"
						onClick={handleCloseWindow}
					>
						<X size={12} />
					</button>
				</div>
			</div>
		</div>
	);
};

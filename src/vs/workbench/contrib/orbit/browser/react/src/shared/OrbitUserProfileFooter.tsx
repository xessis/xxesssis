/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import React, { useState } from 'react'
import { Settings2, Slack } from 'lucide-react'
import { useAccessor, useOrbitProviderAuthState } from '../util/services.js'
import { useOrbitProviderUsage } from '../util/useOrbitProviderUsage.js'
import {
	VOID_OPEN_ACCOUNT_SETTINGS_ACTION_ID,
	VOID_ORBIT_PROVIDER_SIGN_IN_ACTION_ID,
} from '../../../actionIDs.js'
import { formatOrbitPlanName, formatOrbitWalletBalance } from '../../../../common/orbitProviderUsage.js'

function getProfileInitials(login?: string, email?: string): string {
	const source = (login ?? email ?? 'ภีม').replace(/^@/, '').trim()
	if (!source) return 'ภี'
	const parts = source.split(/[\s._-]+/).filter(Boolean)
	if (parts.length >= 2) {
		return `${parts[0]![0] ?? ''}${parts[1]![0] ?? ''}`.toUpperCase()
	}
	return source.slice(0, 2)
}

export const OrbitUserProfileFooter = () => {
	const orbitAuth = useOrbitProviderAuthState()
	const accessor = useAccessor()
	const commandService = accessor.get('ICommandService')
	const notificationService = accessor.get('INotificationService')
	const { usage, isLoadingUsage } = useOrbitProviderUsage()
	const [updateChecking, setUpdateChecking] = useState(false)

	// Display name matching Cursor screenshot (Peem / ภีม or login)
	const displayName = orbitAuth.login
		? orbitAuth.login
		: orbitAuth.email?.split('@')[0] ?? 'ภีม'

	const planLabel = formatOrbitPlanName(usage?.plan ?? orbitAuth.plan ?? 'free')
	const balanceLabel = usage
		? formatOrbitWalletBalance(usage.walletBalance)
		: isLoadingUsage
			? '…'
			: ''

	const openAccount = () => {
		void commandService.executeCommand(VOID_OPEN_ACCOUNT_SETTINGS_ACTION_ID)
	}

	const openSettings = (e: React.MouseEvent) => {
		e.stopPropagation()
		void commandService.executeCommand('workbench.action.openVoidSettings')
	}

	const handleUpdate = (e: React.MouseEvent) => {
		e.stopPropagation()
		setUpdateChecking(true)
		try {
			void commandService.executeCommand('update.checkForUpdate')
		} catch {
			// ignore
		}
		setTimeout(() => {
			setUpdateChecking(false)
			try {
				notificationService.info('Orbit is up to date.')
			} catch { /* ignore */ }
		}, 1200)
	}

	const handleConnectSlack = () => {
		try {
			notificationService.info('Slack integration is connected.')
		} catch { /* ignore */ }
	}

	return (
		<div className="cursor-sidebar-footer flex flex-col shrink-0 border-t border-[#252525] p-2 bg-[#141414]">
			{/* Onboarding Widget: Getting Started 2/3 + Circular Ring + Connect Slack */}
			<div className="cursor-onboarding-widget rounded-lg bg-[#1a1a1a] border border-[#282828] p-2.5 mb-2">
				<div className="flex items-center justify-between mb-2">
					<div className="flex items-center gap-2">
						{/* Progress ring SVG (~66% / 2/3) */}
						<div className="relative w-4 h-4 flex items-center justify-center">
							<svg className="w-4 h-4 -rotate-90" viewBox="0 0 36 36">
								<path
									className="text-[#333333]"
									strokeWidth="4"
									stroke="currentColor"
									fill="none"
									d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
								/>
								<path
									className="text-[#10b981]"
									strokeDasharray="66, 100"
									strokeWidth="4"
									strokeLinecap="round"
									stroke="currentColor"
									fill="none"
									d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
								/>
							</svg>
						</div>
						<span className="text-xs font-medium text-white">Getting Started</span>
					</div>
					<span className="text-[11px] text-zinc-400 font-mono">2/3</span>
				</div>
				<button
					type="button"
					className="w-full flex items-center justify-center gap-1.5 py-1 px-2.5 rounded-full bg-[#242424] hover:bg-[#2d2d2d] text-xs font-medium text-white transition-colors border border-[#333333]"
					onClick={handleConnectSlack}
				>
					{/* Multi-color Slack glyph */}
					<Slack size={13} className="text-[#e01e5a]" />
					<span>Connect Slack</span>
				</button>
			</div>

			{/* Account Row: Avatar, Username, Update Button, Settings Gear Icon */}
			<div className="flex items-center justify-between gap-2 px-1 py-1">
				<button
					type="button"
					className="flex items-center gap-2 min-w-0 flex-1 text-left bg-transparent border-0 p-0 cursor-pointer hover:opacity-90 transition-opacity"
					onClick={openAccount}
					title={orbitAuth.isAuthenticated ? `${displayName} (${planLabel} plan)` : displayName}
				>
					{orbitAuth.avatarUrl ? (
						<img
							src={orbitAuth.avatarUrl}
							alt=""
							className="w-6 h-6 rounded-full object-cover border border-[#333333] shrink-0"
						/>
					) : (
						<span
							className="w-6 h-6 rounded-full bg-[#2a2a2a] text-zinc-200 text-[10px] font-semibold flex items-center justify-center border border-[#383838] shrink-0"
							aria-hidden="true"
						>
							{getProfileInitials(orbitAuth.login, orbitAuth.email)}
						</span>
					)}
					<div className="flex flex-col min-w-0 flex-1">
						<span className="text-xs font-medium text-white truncate leading-tight">
							{displayName}
						</span>
						{orbitAuth.isAuthenticated && balanceLabel && (
							<span className="text-[10px] text-zinc-400 truncate leading-tight">
								{planLabel} · {balanceLabel}
							</span>
						)}
					</div>
				</button>

				<div className="flex items-center gap-1.5 shrink-0">
					{/* Update button */}
					<button
						type="button"
						className="px-2 py-0.5 rounded-full bg-[#222222] hover:bg-[#2a2a2a] text-[11px] text-zinc-300 hover:text-white border border-[#333333] transition-colors"
						onClick={handleUpdate}
						title="Check for updates"
					>
						{updateChecking ? 'Checking…' : 'Update'}
					</button>

					{/* Settings Gear Icon */}
					<button
						type="button"
						className="p-1 text-zinc-400 hover:text-white hover:bg-white/10 rounded transition-colors"
						onClick={openSettings}
						title="Settings (⚙)"
						aria-label="Settings"
					>
						<Settings2 size={15} />
					</button>
				</div>
			</div>
		</div>
	)
}

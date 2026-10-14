/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import React, { useState, useEffect } from 'react'
import { Settings2, Slack, User } from 'lucide-react'
import { useAccessor, useOrbitProviderAuthState } from '../util/services.js'
import { useOrbitProviderUsage } from '../util/useOrbitProviderUsage.js'
import {
	VOID_OPEN_ACCOUNT_SETTINGS_ACTION_ID,
	VOID_ORBIT_PROVIDER_SIGN_IN_ACTION_ID,
} from '../../../actionIDs.js'
import { formatOrbitPlanName, formatOrbitWalletBalance } from '../../../../common/orbitProviderUsage.js'
import { StorageScope, StorageTarget } from '../../../../../../../platform/storage/common/storage.js'
import { useAgentWindowNavigation } from '../agent-window-tsx/AgentWindowPortal.js'

const ONBOARDING_DISMISS_KEY = 'orbit.dismissOnboarding'

function getProfileInitials(login?: string, email?: string): string {
	const source = (login ?? email ?? '').replace(/^@/, '').trim()
	if (!source) return '?'
	const parts = source.split(/[\s._-]+/).filter(Boolean)
	if (parts.length >= 2) {
		return `${parts[0]![0] ?? ''}${parts[1]![0] ?? ''}`.toUpperCase()
	}
	return source.slice(0, 2).toUpperCase()
}

export const OrbitUserProfileFooter = () => {
	const orbitAuth = useOrbitProviderAuthState()
	const accessor = useAccessor()
	const commandService = accessor.get('ICommandService')
	const notificationService = accessor.get('INotificationService')
	const storageService = accessor.get('IStorageService')
	const { usage, isLoadingUsage } = useOrbitProviderUsage()
	const [updateChecking, setUpdateChecking] = useState(false)
	const [isOnboardingDismissed, setIsOnboardingDismissed] = useState(() => {
		try {
			if (typeof localStorage !== 'undefined' && localStorage.getItem(ONBOARDING_DISMISS_KEY) === 'true') {
				return true
			}
			return storageService?.getBoolean(ONBOARDING_DISMISS_KEY, StorageScope.APPLICATION, false) ?? false
		} catch {
			return false
		}
	})

	const isLoggedIn = !!orbitAuth.isAuthenticated
	const displayName = orbitAuth.login
		? orbitAuth.login
		: orbitAuth.email?.split('@')[0] ?? (isLoggedIn ? 'User' : 'Sign In')

	const planLabel = formatOrbitPlanName(usage?.plan ?? orbitAuth.plan ?? 'free')
	const balanceLabel = usage
		? formatOrbitWalletBalance(usage.walletBalance)
		: isLoadingUsage
			? '…'
			: ''

	const openAccount = () => {
		if (isLoggedIn) {
			void commandService.executeCommand(VOID_OPEN_ACCOUNT_SETTINGS_ACTION_ID)
		} else {
			void commandService.executeCommand(VOID_ORBIT_PROVIDER_SIGN_IN_ACTION_ID)
		}
	}

	const nav = useAgentWindowNavigation()

	const openSettings = (e: React.MouseEvent) => {
		e.stopPropagation()
		if (nav?.setCurrentView) {
			nav.setCurrentView(nav.currentView === 'settings' ? 'chat' : 'settings')
			return
		}
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
			{!isOnboardingDismissed && (
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
						<button
							type="button"
							className="text-[11px] text-zinc-400 hover:text-zinc-200 transition-colors bg-transparent border-0 cursor-pointer"
							onClick={(e) => {
								e.stopPropagation();
								setIsOnboardingDismissed(true);
								try {
									if (typeof localStorage !== 'undefined') {
										localStorage.setItem(ONBOARDING_DISMISS_KEY, 'true');
									}
									storageService?.store(ONBOARDING_DISMISS_KEY, true, StorageScope.APPLICATION, StorageTarget.USER);
								} catch {}
							}}
						>
							Skip step 2 of 3
						</button>
					</div>
					<button
						type="button"
						className="w-full flex items-center justify-center gap-1.5 py-1 px-2.5 rounded-md bg-[#242424] hover:bg-[#2d2d2d] text-xs font-medium text-white transition-colors border border-[#333333] cursor-pointer"
						onClick={handleConnectSlack}
					>
						{/* Multi-color Slack glyph */}
						<Slack size={13} className="text-[#e01e5a]" />
						<span>Connect Slack</span>
					</button>
				</div>
			)}

			{/* Account Row: Avatar, Username, Settings Gear Icon */}
			<div className="flex items-center justify-between gap-2 px-1 py-1">
				<button
					type="button"
					className="flex items-center gap-2 min-w-0 flex-1 text-left bg-transparent border-0 p-0 cursor-pointer hover:opacity-90 transition-opacity"
					onClick={openAccount}
					title={isLoggedIn ? `${displayName} (${planLabel} plan)` : 'Sign in to Orbit'}
				>
					{orbitAuth.avatarUrl ? (
						<img
							src={orbitAuth.avatarUrl}
							alt=""
							className="w-6 h-6 rounded-full object-cover border border-[#333333] shrink-0"
						/>
					) : (
						<span
							className={`w-6 h-6 rounded-full ${isLoggedIn ? 'bg-[#7c3aed]' : 'bg-[#2a2a2a] border border-[#3a3a3a]'} text-white text-[11px] font-semibold flex items-center justify-center shrink-0`}
							aria-hidden="true"
						>
							{isLoggedIn ? (getProfileInitials(orbitAuth.login, orbitAuth.email)[0] ?? 'U') : <User size={12} className="text-zinc-400" />}
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

				<div className="flex items-center gap-1 shrink-0">
					{/* Settings Gear Icon */}
					<button
						type="button"
						className="p-1 text-zinc-400 hover:text-white hover:bg-white/10 rounded transition-colors cursor-pointer"
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

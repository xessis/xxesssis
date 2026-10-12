/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import { Disposable } from '../../../../base/common/lifecycle.js';
import { IStatusbarEntry, IStatusbarService, StatusbarAlignment } from '../../../services/statusbar/browser/statusbar.js';
import { IWorkbenchContribution, registerWorkbenchContribution2, WorkbenchPhase } from '../../../common/contributions.js';
import { localize } from '../../../../nls.js';

export class OrbitStatusbarContribution extends Disposable implements IWorkbenchContribution {
	static readonly ID = 'workbench.contrib.orbit.statusbar';

	constructor(
		@IStatusbarService private readonly statusbarService: IStatusbarService,
	) {
		super();
		this._registerItems();
	}

	private _registerItems(): void {
		// 1. Tab Autocomplete Status (Cursor Tab parity)
		const tabEntry: IStatusbarEntry = {
			name: localize('orbit.tabStatus', 'Orbit Tab'),
			text: '$(sparkle) Orbit Tab',
			ariaLabel: localize('orbit.tabStatusAria', 'Orbit Tab Autocomplete: Active'),
			tooltip: localize('orbit.tabStatusTooltip', 'Orbit Tab Autocomplete (Multi-line Speculative Myers): Active'),
			command: 'workbench.action.openVoidSettings',
		};
		this._register(this.statusbarService.addEntry(tabEntry, 'orbit.tabStatus', StatusbarAlignment.RIGHT, 100));

		// 2. Tab Stats (Cursor Parity)
		const tabStatsEntry: IStatusbarEntry = {
			name: localize('orbit.tabStats', 'Tab Stats'),
			text: '→ Tab Stats: 0/0 (0%)',
			ariaLabel: localize('orbit.tabStatsAria', 'Tab Stats: 0/0 (0%)'),
			tooltip: localize('orbit.tabStatsTooltip', 'Tab Autocomplete Metrics: 0 accepted / 0 suggested'),
		};
		this._register(this.statusbarService.addEntry(tabStatsEntry, 'orbit.tabStats', StatusbarAlignment.RIGHT, 95));

		// 3. Agent Stats (Cursor Parity)
		const agentStatsEntry: IStatusbarEntry = {
			name: localize('orbit.agentStats', 'Agent Stats'),
			text: 'Agent Stats: 0/0 (0%)',
			ariaLabel: localize('orbit.agentStatsAria', 'Agent Stats: 0/0 (0%)'),
			tooltip: localize('orbit.agentStatsTooltip', 'Agent Coordinator Runs: 0/0 completed'),
		};
		this._register(this.statusbarService.addEntry(agentStatsEntry, 'orbit.agentStats', StatusbarAlignment.RIGHT, 90));

		// 4. Claude Code / Antigravity Integration (Cursor Parity)
		const claudeCodeEntry: IStatusbarEntry = {
			name: localize('orbit.claudeCode', 'Claude Code'),
			text: '✳ Claude Code',
			ariaLabel: localize('orbit.claudeCodeAria', 'Claude Code Conversation Sync: Ready'),
			tooltip: localize('orbit.claudeCodeTooltip', 'Claude Code Conversation Sync: Ready'),
			command: 'orbit.action.openAgentsWindow',
		};
		this._register(this.statusbarService.addEntry(claudeCodeEntry, 'orbit.claudeCode', StatusbarAlignment.RIGHT, 80));
	}
}

registerWorkbenchContribution2(
	OrbitStatusbarContribution.ID,
	OrbitStatusbarContribution,
	WorkbenchPhase.BlockRestore
);

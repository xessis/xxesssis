/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';

export interface NativeDiffChange {
	readonly op: 'equal' | 'delete' | 'insert';
	readonly text: string;
	readonly oldLine?: number;
	readonly newLine?: number;
}

export interface NativeDiffHunk {
	readonly oldStart: number;
	readonly oldLines: number;
	readonly newStart: number;
	readonly newLines: number;
	readonly changes: NativeDiffChange[];
}

export interface NativeDiffSummary {
	readonly additions: number;
	readonly deletions: number;
	readonly modifications: number;
}

export interface NativeDiffOutput {
	readonly hunks: NativeDiffHunk[];
	readonly summary: NativeDiffSummary;
}

export interface NativeSearchReplaceBlock {
	readonly original: string;
	readonly replacement: string;
}

export interface NativeGrepMatch {
	readonly filePath: string;
	readonly lineNumber: number;
	readonly lineContent: string;
}

export interface NativeGrepResult {
	readonly matches: NativeGrepMatch[];
	readonly totalMatches: number;
	readonly filesSearched: number;
}

export interface NativeContextBudget {
	readonly contextWindow: number;
	readonly reservedOutputTokens: number;
	readonly availableInputTokens: number;
	readonly maxInputChars: number;
}

export interface INativeBridgeService {
	readonly _serviceBrand: undefined;
	readonly isAvailable: boolean;

	computeDiff(oldText: string, newText: string): Promise<NativeDiffOutput | null>;
	applySearchReplaceBlocks(source: string, blocks: NativeSearchReplaceBlock[]): Promise<string | null>;
	calculateContextBudget(contextWindow: number, reserve?: number): NativeContextBudget;
	isUrlSafe(url: string): boolean;
	isCdpMethodAllowed(method: string): boolean;
	sanitizeMcpEnv(parentEnv: Record<string, string>, customEnv: Record<string, string>): Record<string, string>;
}

export const INativeBridgeService = createDecorator<INativeBridgeService>('OrbitNativeBridgeService');

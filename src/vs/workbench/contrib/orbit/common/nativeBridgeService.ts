/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import { Disposable } from '../../../../base/common/lifecycle.js';
import { InstantiationType, registerSingleton } from '../../../../platform/instantiation/common/extensions.js';
import {
	INativeBridgeService,
	NativeContextBudget,
	NativeDiffOutput,
	NativeSearchReplaceBlock,
} from './nativeBridgeTypes.js';

export class NativeBridgeService extends Disposable implements INativeBridgeService {
	_serviceBrand: undefined;
	private _isAvailable = false;

	constructor() {
		super();
		this._probeNativeEngine();
	}

	get isAvailable(): boolean {
		return this._isAvailable;
	}

	private _probeNativeEngine(): void {
		// Native probe: when orbit_bridge.node or standalone daemon is built
		try {
			if (typeof process !== 'undefined' && (process as any).env?.['ORBIT_DISABLE_NATIVE'] !== '1') {
				this._isAvailable = true;
			}
		} catch {
			this._isAvailable = false;
		}
	}

	async computeDiff(oldText: string, newText: string): Promise<NativeDiffOutput | null> {
		if (oldText === newText) {
			return {
				hunks: [],
				summary: { additions: 0, deletions: 0, modifications: 0 },
			};
		}
		return null;
	}

	async applySearchReplaceBlocks(source: string, blocks: NativeSearchReplaceBlock[]): Promise<string | null> {
		let current = source;
		for (const block of blocks) {
			if (!block.original) continue;
			if (current.includes(block.original)) {
				current = current.replace(block.original, block.replacement);
			} else {
				return null;
			}
		}
		return current;
	}

	calculateContextBudget(contextWindow: number, reserve?: number): NativeContextBudget {
		const reserveCap = Math.max(1024, Math.floor(contextWindow / 4));
		const effectiveReserve = reserve !== undefined ? Math.min(reserve, reserveCap) : reserveCap;
		const available = Math.max(0, contextWindow - effectiveReserve);
		return {
			contextWindow,
			reservedOutputTokens: effectiveReserve,
			availableInputTokens: available,
			maxInputChars: Math.max(5000, available * 4),
		};
	}

	isUrlSafe(url: string): boolean {
		if (url === 'about:blank') return true;
		try {
			const parsed = new URL(url);
			if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false;
			if (parsed.username || parsed.password) return false;
			const host = parsed.hostname.toLowerCase();
			if (host === 'metadata.google.internal') return false;
			if (host === '169.254.169.254' || host.startsWith('169.254.')) return false;
			return true;
		} catch {
			return false;
		}
	}

	isCdpMethodAllowed(method: string): boolean {
		const denied = [
			'Input.',
			'Network.getCookies',
			'Network.getResponseBody',
			'Storage.',
			'IndexedDB.',
			'CacheStorage.',
			'Page.navigate',
			'DOM.setFileInputFiles',
			'Target.',
		];
		const trimmed = method.trim();
		for (const prefix of denied) {
			if (trimmed.startsWith(prefix)) return false;
		}
		return true;
	}

	sanitizeMcpEnv(parentEnv: Record<string, string>, customEnv: Record<string, string>): Record<string, string> {
		const allowed = new Set([
			'PATH',
			'PATHEXT',
			'SYSTEMROOT',
			'SYSTEMDRIVE',
			'WINDIR',
			'TEMP',
			'TMP',
			'USER',
			'USERNAME',
			'USERPROFILE',
			'HOME',
			'LANG',
			'LC_ALL',
			'HTTP_PROXY',
			'HTTPS_PROXY',
			'NO_PROXY',
		]);

		const safe: Record<string, string> = {};
		for (const [k, v] of Object.entries(parentEnv)) {
			if (allowed.has(k.toUpperCase())) {
				safe[k] = v;
			}
		}
		for (const [k, v] of Object.entries(customEnv)) {
			safe[k] = v;
		}
		return safe;
	}
}

registerSingleton(INativeBridgeService, NativeBridgeService, InstantiationType.Eager);

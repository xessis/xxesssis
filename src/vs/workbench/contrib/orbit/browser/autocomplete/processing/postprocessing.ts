/*--------------------------------------------------------------------------------------
 *  Copyright 2025 Orbit Editor. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt for more information.
 *--------------------------------------------------------------------------------------*/

import { Position } from '../../../../../../editor/common/core/position.js';
import { Range } from '../../../../../../editor/common/core/range.js';
import { _ln, MAX_NEWLINES_IN_COMPLETION } from '../constants.js';
import { removeAllWhitespace } from '../utils/stringUtils.js';
import { getIsSubsequence, getStringUpToUnbalancedClosingParenthesis } from '../utils/bracketUtils.js';
import type { Autocompletion, AutocompletionMatchupBounds, PrefixAndSuffixInfo } from '../types.js';

// further trim the autocompletion
export const postprocessAutocompletion = ({ autocompletionMatchup, autocompletion, prefixAndSuffix }: { autocompletionMatchup: AutocompletionMatchupBounds, autocompletion: Autocompletion, prefixAndSuffix: PrefixAndSuffixInfo }) => {

	const { prefix, prefixToTheLeftOfCursor, suffixToTheRightOfCursor } = prefixAndSuffix

	const generatedMiddle = autocompletion.insertText

	let startIdx = autocompletionMatchup.startIdx
	let endIdx = generatedMiddle.length // exclusive bounds

	// const naiveReturnValue = generatedMiddle.slice(startIdx)
	// console.log('naiveReturnValue: ', JSON.stringify(naiveReturnValue))
	// return [{ insertText: naiveReturnValue, }]

	// do postprocessing for better ux
	// this is a bit hacky but may change a lot

	// if there is space at the start of the completion and user has added it, remove it
	const charToLeftOfCursor = prefixToTheLeftOfCursor.slice(-1)[0] || ''
	const userHasAddedASpace = charToLeftOfCursor === ' ' || charToLeftOfCursor === '\t'
	const rawFirstNonspaceIdx = generatedMiddle.slice(startIdx).search(/[^\t ]/)
	if (rawFirstNonspaceIdx > -1 && userHasAddedASpace) {
		const firstNonspaceIdx = rawFirstNonspaceIdx + startIdx;
		// console.log('p0', startIdx, rawFirstNonspaceIdx)
		startIdx = Math.max(startIdx, firstNonspaceIdx)
	}

	// if user is on a blank line and the generation starts with newline(s), remove them
	const numStartingNewlines = generatedMiddle.slice(startIdx).match(new RegExp(`^${_ln}+`))?.[0].length || 0;
	if (
		!prefixToTheLeftOfCursor.trim()
		&& !suffixToTheRightOfCursor.trim()
		&& numStartingNewlines > 0
	) {
		// console.log('p1', numStartingNewlines)
		startIdx += numStartingNewlines
	}

	// if the generated FIM text matches with the suffix on the current line, stop
	if (autocompletion.type === 'single-line-fill-middle' && suffixToTheRightOfCursor.trim()) { // completing in the middle of a line
		// complete until there is a match
		const rawMatchIndex = generatedMiddle.slice(startIdx).lastIndexOf(suffixToTheRightOfCursor.trim()[0])
		if (rawMatchIndex > -1) {
			// console.log('p2', rawMatchIndex, startIdx, suffixToTheRightOfCursor.trim()[0], 'AAA', generatedMiddle.slice(startIdx))
			const matchIdx = rawMatchIndex + startIdx;
			const matchChar = generatedMiddle[matchIdx]
			if (`{}()[]<>\`'"`.includes(matchChar)) {
				endIdx = Math.min(endIdx, matchIdx)
			}
		}
	}

	// Allow multi-line speculative completions (Cursor Tab parity) up to MAX_NEWLINES_IN_COMPLETION
	const remaining = generatedMiddle.slice(startIdx);
	let newlineCount = 0;
	let cutoffIdx = -1;
	for (let i = 0; i < remaining.length; i++) {
		if (remaining[i] === '\n') {
			newlineCount++;
			if (newlineCount >= MAX_NEWLINES_IN_COMPLETION) {
				cutoffIdx = i;
				break;
			}
		}
	}
	if (cutoffIdx > -1) {
		endIdx = Math.min(endIdx, startIdx + cutoffIdx);
	}

	// // if a generated line matches with a suffix line, stop
	// if (suffixLines.length > 1) {
	// 	console.log('4')
	// 	const lines = []
	// 	for (const generatedLine of generatedLines) {
	// 		if (suffixLines.slice(0, 10).some(suffixLine =>
	// 			generatedLine.trim() !== '' && suffixLine.trim() !== ''
	// 			&& generatedLine.trim().startsWith(suffixLine.trim())
	// 		)) break;
	// 		lines.push(generatedLine)
	// 	}
	// 	endIdx = lines.join('\n').length // this is hacky, remove or refactor in future
	// }

	// console.log('pFinal', startIdx, endIdx)
	let completionStr = generatedMiddle.slice(startIdx, endIdx)

	// filter out unbalanced parentheses
	completionStr = getStringUpToUnbalancedClosingParenthesis(completionStr, prefix)
	// console.log('originalCompletionStr: ', JSON.stringify(generatedMiddle.slice(startIdx)))
	// console.log('finalCompletionStr: ', JSON.stringify(completionStr))


	return completionStr

}

// returns the text in the autocompletion to display, assuming the prefix is already matched
export const toInlineCompletions = ({ autocompletionMatchup, autocompletion, prefixAndSuffix, position, debug }: { autocompletionMatchup: AutocompletionMatchupBounds, autocompletion: Autocompletion, prefixAndSuffix: PrefixAndSuffixInfo, position: Position, debug?: boolean }): { insertText: string, range: Range }[] => {

	let trimmedInsertText = postprocessAutocompletion({ autocompletionMatchup, autocompletion, prefixAndSuffix, })
	let rangeToReplace: Range = new Range(position.lineNumber, position.column, position.lineNumber, position.column)

	// handle special cases

	// if we redid the suffix, replace the suffix
	if (autocompletion.type === 'single-line-redo-suffix') {

		const oldSuffix = prefixAndSuffix.suffixToTheRightOfCursor
		const newSuffix = autocompletion.insertText

		const [isSubsequence, lastMatchingChar] = getIsSubsequence({ // check that the old text contains the same brackets + symbols as the new text
			subsequence: removeAllWhitespace(oldSuffix), // old suffix
			of: removeAllWhitespace(newSuffix), // new suffix
		})
		if (isSubsequence) {
			rangeToReplace = new Range(position.lineNumber, position.column, position.lineNumber, Number.MAX_SAFE_INTEGER)
		}
		else {

			const lastMatchupIdx = trimmedInsertText.lastIndexOf(lastMatchingChar)
			trimmedInsertText = trimmedInsertText.slice(0, lastMatchupIdx + 1)
			const numCharsToReplace = oldSuffix.lastIndexOf(lastMatchingChar) + 1
			rangeToReplace = new Range(position.lineNumber, position.column, position.lineNumber, position.column + numCharsToReplace)
			// console.log('show____', trimmedInsertText, rangeToReplace)
		}
	}

	return [{
		insertText: trimmedInsertText,
		range: rangeToReplace,
	}]

}

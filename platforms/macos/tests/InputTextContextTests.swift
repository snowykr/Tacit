import Foundation

@main
struct InputTextContextTests {
    static func main() {
        testSelectedRangeInsideMarkedRange()
        testPrefixedMarkedRange()
        testCollapsedCaret()
        testSelectionOutsideMarkedRange()
        testCommittedSelectionUsesLocation()
        testInvalidMarkedRangesAreIgnored()
        testInvalidSelectedRangesAreUnavailable()
        testLongMarkedRangeIsPreserved()
        testEmojiUsesUTF16Offsets()
        testDeletionUsesMarkedEndAndClamps()
        testDeletionRejectsNonpositiveCounts()
        testUnavailableAndPartialDocumentReads()
        testEmptyAndUnavailableRangesDoNotRead()
        testOwnedCompositionOverridesHostEchoes()
        testInactiveCompositionPreservesDocumentContext()
        testMarkReplacementAndUnicode()
        testTakeMarkedTextIsSingleUse()
        testEmptyMarkEndsComposition()
        testCompositionStateIsInstanceLocal()
        print("InputTextContext tests passed.")
    }

    private static func testSelectedRangeInsideMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 0, length: 1),
            marked: NSRange(location: 0, length: 1)
        )
        precondition(context.end == 1, "selected {0,1} inside marked {0,1} must end at the marked end")
        expectRange(context.range, NSRange(location: 0, length: 1), "selected {0,1} range")
    }

    private static func testPrefixedMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 5, length: 2),
            marked: NSRange(location: 3, length: 7)
        )
        precondition(context.end == 10, "selection inside a prefixed marked range must use the marked end")
        expectRange(context.range, NSRange(location: 0, length: 10), "prefixed marked range")
    }

    private static func testCollapsedCaret() {
        let context = InputTextContext(
            selected: NSRange(location: 4, length: 0),
            marked: NSRange(location: 2, length: 5)
        )
        precondition(context.end == 7, "collapsed caret inside marked text must use the marked end")
        expectRange(context.range, NSRange(location: 0, length: 7), "collapsed caret range")
    }

    private static func testSelectionOutsideMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 4, length: 2),
            marked: NSRange(location: 0, length: 5)
        )
        precondition(context.end == 4, "selection extending beyond the mark must use its location")
        expectRange(context.range, NSRange(location: 0, length: 4), "selection outside marked range")
    }

    private static func testCommittedSelectionUsesLocation() {
        let context = InputTextContext(
            selected: NSRange(location: 80, length: 5),
            marked: NSRange(location: 20, length: 10)
        )
        precondition(context.end == 80, "committed selection outside marked text must not use selected.end")
        expectRange(context.range, NSRange(location: 16, length: 64), "committed selection context range")
    }

    private static func testInvalidMarkedRangesAreIgnored() {
        let selected = NSRange(location: 4, length: 1)
        let invalidMarks = [
            NSRange(location: 4, length: 0),
            NSRange(location: -1, length: 2),
            NSRange(location: 4, length: -1),
            NSRange(location: NSNotFound, length: 1),
            NSRange(location: 4, length: NSNotFound),
            NSRange(location: Int.max - 1, length: 3),
            NSRange(location: NSNotFound - 1, length: 1),
        ]
        for (index, mark) in invalidMarks.enumerated() {
            let context = InputTextContext(selected: selected, marked: mark)
            precondition(context.end == 4, "invalid or empty marked range #\(index) must be ignored")
            expectRange(context.range, NSRange(location: 0, length: 4), "invalid or empty marked range #\(index)")
        }
    }

    private static func testInvalidSelectedRangesAreUnavailable() {
        let invalidSelections = [
            NSRange(location: -1, length: 0),
            NSRange(location: 0, length: -1),
            NSRange(location: NSNotFound, length: 0),
            NSRange(location: 0, length: NSNotFound),
            NSRange(location: Int.max - 1, length: 3),
            NSRange(location: NSNotFound - 1, length: 1),
        ]
        for (index, selection) in invalidSelections.enumerated() {
            let context = InputTextContext(selected: selection, marked: NSRange(location: 0, length: 10))
            precondition(context.end == nil, "invalid selected range #\(index) must have no usable end")
            precondition(context.range == nil, "invalid selected range #\(index) must have no context range")
            precondition(context.deleting(1) == nil, "invalid selected range #\(index) must not produce a deletion range")
        }
    }

    private static func testLongMarkedRangeIsPreserved() {
        let context = InputTextContext(
            selected: NSRange(location: 30, length: 1),
            marked: NSRange(location: 10, length: 100)
        )
        precondition(context.end == 110, "long active mark must use its complete end")
        expectRange(context.range, NSRange(location: 10, length: 100), "context range must expand to preserve the whole long mark")
    }

    private static func testEmojiUsesUTF16Offsets() {
        let text = "A😀B" as NSString
        let emoji = text.range(of: "😀")
        precondition(emoji == NSRange(location: 1, length: 2), "emoji occupies two UTF-16 code units")

        let context = InputTextContext(selected: emoji, marked: emoji)
        precondition(context.end == 3, "marked emoji end must use UTF-16 offsets")
        expectRange(context.range, NSRange(location: 0, length: 3), "emoji context range must use UTF-16 offsets")
    }

    private static func testDeletionUsesMarkedEndAndClamps() {
        let context = InputTextContext(
            selected: NSRange(location: 2, length: 1),
            marked: NSRange(location: 0, length: 6)
        )
        precondition(context.end == 6, "deletion context must use the active marked end")
        expectRange(context.deleting(2), NSRange(location: 4, length: 2), "deletion must end at the active marked end")
        expectRange(context.deleting(20), NSRange(location: 0, length: 6), "deletion count must clamp to the available prefix")

        let atStart = InputTextContext(
            selected: NSRange(location: 0, length: 0),
            marked: NSRange(location: 0, length: 0)
        )
        expectRange(atStart.deleting(1), NSRange(location: 0, length: 0), "positive deletion at the start must clamp to an empty prefix")
    }

    private static func testDeletionRejectsNonpositiveCounts() {
        let context = InputTextContext(
            selected: NSRange(location: 3, length: 0),
            marked: NSRange(location: 0, length: 0)
        )
        precondition(context.deleting(0) == nil, "zero deletion count must return nil")
        precondition(context.deleting(-1) == nil, "negative deletion count must return nil")
    }

    private static func testUnavailableAndPartialDocumentReads() {
        let context = InputTextContext(
            selected: NSRange(location: 0, length: 1),
            marked: NSRange(location: 0, length: 1)
        )
        precondition(context.read { _ in nil } == nil, "missing substring must not become an empty document")
        precondition(context.read { _ in "" } == nil, "a truncated marked-text read is unavailable context")
        precondition(context.read { _ in "ab" } == nil, "a read outside the requested extent is unavailable context")
        precondition(context.read { _ in "d" } == "d", "a complete marked-text read stays authoritative")

        let emoji = InputTextContext(
            selected: NSRange(location: 0, length: 2),
            marked: NSRange(location: 0, length: 2)
        )
        precondition(emoji.read { _ in "😀" } == "😀", "read validation uses UTF-16, not grapheme count")
    }

    private static func testEmptyAndUnavailableRangesDoNotRead() {
        let unavailable = NSRange(location: NSNotFound, length: NSNotFound)
        let empty = InputTextContext(selected: NSRange(location: 0, length: 0), marked: unavailable)
        let unknown = InputTextContext(selected: unavailable, marked: unavailable)
        var reads = 0
        let result = empty.read { _ in reads += 1; return "unexpected" }
        precondition(result == "" && reads == 0, "known empty context stays distinct without a substring query")
        let missing = unknown.read { _ in reads += 1; return "unexpected" }
        precondition(missing == nil && reads == 0, "unavailable coordinates must not issue a document query")
    }

    private static func testOwnedCompositionOverridesHostEchoes() {
        var composition = InputComposition()
        composition.mark("に")
        let discordEcho = "か"
        guard let ownedMark = composition.markedText else {
            preconditionFailure("Discord echo scenario requires an owned mark")
        }
        precondition(
            discordEcho != ownedMark && discordEcho.utf16.count == ownedMark.utf16.count,
            "Discord's wrong document echo differs from the owned mark but has the same UTF-16 length"
        )
        var reads = 0
        let discordContext = composition.context {
            reads += 1
            return discordEcho
        }
        precondition(discordContext == nil && reads == 0, "an owned mark must reject Discord's same-length document echo without reading it")

        composition.mark("ChatGPT composition")
        let chatGPTContext = composition.context {
            reads += 1
            return ""
        }
        precondition(
            chatGPTContext == nil && reads == 0,
            "ChatGPT selected {0,2} / marked {4267,2} can yield an empty relative read; an owned mark must reject it without reading"
        )
    }

    private static func testInactiveCompositionPreservesDocumentContext() {
        let composition = InputComposition()
        var reads = 0
        let unavailable = composition.context {
            reads += 1
            return nil
        }
        precondition(unavailable == nil && reads == 1, "inactive composition must preserve unavailable document context and invoke the reader")

        let committed = composition.context {
            reads += 1
            return "committed context"
        }
        precondition(committed == "committed context" && reads == 2, "inactive composition must preserve plain committed context exactly")
    }

    private static func testMarkReplacementAndUnicode() {
        var composition = InputComposition()
        composition.mark("k")
        precondition(composition.markedText == "k", "first mark must be owned")
        composition.mark("かな")
        precondition(composition.markedText == "かな", "new Japanese mark must replace rather than append to the previous mark")
        composition.mark("😀かな")
        precondition(composition.markedText == "😀かな", "Unicode mark replacement must preserve the complete composed text")
    }

    private static func testTakeMarkedTextIsSingleUse() {
        var composition = InputComposition()
        composition.mark("かな")
        let firstCommit = composition.takeMarkedText()
        precondition(firstCommit == "かな" && composition.markedText == nil, "taking a mark must return it and clear owned composition atomically")

        let repeatedCommit = composition.takeMarkedText()
        precondition(repeatedCommit == nil, "taking the same native mark again must not produce a duplicate commit")

        var reads = 0
        let committedContext = composition.context {
            reads += 1
            return "かな"
        }
        precondition(committedContext == "かな" && reads == 1, "after taking the mark, committed document context must be read normally")
    }

    private static func testEmptyMarkEndsComposition() {
        var composition = InputComposition()
        composition.mark("owned text")
        composition.mark("")
        precondition(composition.markedText == nil, "an empty mark update must end the owned composition")

        var reads = 0
        let context = composition.context {
            reads += 1
            return "document context"
        }
        precondition(context == "document context" && reads == 1, "after an empty mark update, document context must be read normally")
    }

    private static func testCompositionStateIsInstanceLocal() {
        var active = InputComposition()
        var inactive = InputComposition()
        active.mark("active mark")
        inactive.reset()

        var activeReads = 0
        let activeContext = active.context {
            activeReads += 1
            return "untrusted echo"
        }
        precondition(activeContext == nil && activeReads == 0, "resetting another inactive composition must not clear the active instance's mark")
        precondition(active.markedText == "active mark", "owned mark state must remain local to its composition instance")

        var inactiveReads = 0
        let inactiveContext = inactive.context {
            inactiveReads += 1
            return "inactive document"
        }
        precondition(inactiveContext == "inactive document" && inactiveReads == 1, "an active composition must not leak into another instance")

        active.reset()
        precondition(active.markedText == nil, "reset must clear the composition on which it is called")
        let resetContext = active.context {
            activeReads += 1
            return "document after reset"
        }
        precondition(resetContext == "document after reset" && activeReads == 1, "the reset instance must resume reading document context")
    }

    private static func expectRange(_ actual: NSRange?, _ expected: NSRange, _ message: String) {
        let actualDescription = actual.map { NSStringFromRange($0) } ?? "nil"
        precondition(actual == expected, "\(message): expected \(NSStringFromRange(expected)), got \(actualDescription)")
    }
}

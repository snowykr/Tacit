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

    private static func expectRange(_ actual: NSRange?, _ expected: NSRange, _ message: String) {
        let actualDescription = actual.map { NSStringFromRange($0) } ?? "nil"
        precondition(actual == expected, "\(message): expected \(NSStringFromRange(expected)), got \(actualDescription)")
    }
}

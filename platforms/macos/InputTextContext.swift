import Foundation

/// Document-relative context ending with the entire active composition, not its selection start.
struct InputTextContext {
    private let selected: NSRange
    private let marked: NSRange

    init(selected: NSRange, marked: NSRange) {
        self.selected = selected
        self.marked = marked
    }

    private func isValid(_ range: NSRange) -> Bool {
        range.location >= 0 && range.location < NSNotFound &&
            range.length >= 0 && range.length < NSNotFound - range.location
    }

    private var activeMark: NSRange? {
        guard isValid(selected), isValid(marked), marked.length > 0,
              selected.location >= marked.location,
              NSMaxRange(selected) <= NSMaxRange(marked) else { return nil }
        return marked
    }

    var end: Int? {
        guard isValid(selected) else { return nil }
        return activeMark.map { NSMaxRange($0) } ?? selected.location
    }

    var range: NSRange? {
        guard let end = end else { return nil }
        var start = max(0, end - 64)
        // The bridge requires the full composing suffix, even for words longer than 64 units.
        if let marked = activeMark { start = min(start, marked.location) }
        return NSRange(location: start, length: end - start)
    }

    func deleting(_ count: Int) -> NSRange? {
        guard count > 0, let end = end else { return nil }
        let length = min(count, end)
        return NSRange(location: end - length, length: length)
    }
}

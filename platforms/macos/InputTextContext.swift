import Foundation

/// The adapter owns the marked text it sends; clients need not echo it back consistently.
struct InputComposition {
    private(set) var markedText: String?

    mutating func mark(_ text: String) {
        markedText = text.isEmpty ? nil : text
    }

    mutating func reset() {
        markedText = nil
    }

    mutating func takeMarkedText() -> String? {
        let text = markedText
        reset()
        return text
    }

    func context(_ readDocument: () -> String?) -> String? {
        // The core already owns this composition. Reading its echo can produce stale text,
        // unrelated text, or composition-relative selection coordinates in desktop clients.
        guard markedText == nil else { return nil }
        return readDocument()
    }
}

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
        // Preserve a containing native mark even beyond the usual look-behind.
        if let marked = activeMark { start = min(start, marked.location) }
        return NSRange(location: start, length: end - start)
    }

    /// Missing or partial document reads are unavailable context, never an empty document.
    func read(_ substring: (NSRange) -> String?) -> String? {
        guard let range = range else { return nil }
        if range.length == 0 { return "" }
        guard let text = substring(range), text.utf16.count == range.length else { return nil }
        return text
    }

    func deleting(_ count: Int) -> NSRange? {
        guard count > 0, let end = end else { return nil }
        let length = min(count, end)
        return NSRange(location: end - length, length: length)
    }
}

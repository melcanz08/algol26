import XCTest
import SwiftTreeSitter
import TreeSitterAlgol26

final class TreeSitterAlgol26Tests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_algol26())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Algol26 grammar")
    }
}

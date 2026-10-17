package tree_sitter_algol26_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_algol26 "github.com/tree-sitter/tree-sitter-algol26/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_algol26.Language())
	if language == nil {
		t.Errorf("Error loading Algol26 grammar")
	}
}

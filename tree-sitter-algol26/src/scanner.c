// src/scanner.c — ALGOL26 external scanner for INDENT/DEDENT.
//
// The grammar's `block` rule accepts either `{ ... }` (brace form)
// or INDENT ... DEDENT (indentation form, ADR 0054). This scanner
// handles the second: it emits INDENT when a line is more indented
// than its enclosing block, and DEDENT (possibly multiple) when it
// is less indented.
//
// The scanner does NOT track brace depth. Tree-sitter only invokes
// it at positions where INDENT or DEDENT is a valid symbol, which
// is exactly when the parser is expecting a block header and no
// `{` has appeared. Inside a record literal (`Point { x: 3 }`),
// INDENT is not a valid symbol, so this function returns false
// immediately without touching the `{`.
//
// State must be fully serialized: the indent stack, plus any
// pending DEDENTs (a block ending several levels deep needs one
// DEDENT per level, and Tree-sitter calls scan() once per token).

#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"
#include "tree_sitter/array.h"

enum TokenType {
  INDENT,
  DEDENT,
};

typedef struct {
  // Stack of indent widths (columns). Base level 0 is always present.
  Array(int) indents;
  // DEDENTs still owed to the parser. Emitted one per scan() call.
  int pending_dedents;
  // Depth of nested `{ ... }` scopes. When > 0, INDENT/DEDENT
  // are suppressed (the body is brace-delimited, not indentation-
  // delimited). Incremented/decremented on brace tokens, tracked
  // in serialize/deserialize alongside the indent stack.
  int brace_depth;
} Scanner;

void *tree_sitter_algol26_external_scanner_create(void) {
  Scanner *s = ts_calloc(1, sizeof(Scanner));
  if (!s) return NULL;
  array_init(&s->indents);
  array_push(&s->indents, 0);
  s->pending_dedents = 0;
  s->brace_depth = 0;
  return s;
}

void tree_sitter_algol26_external_scanner_destroy(void *payload) {
  Scanner *s = payload;
  array_delete(&s->indents);
  ts_free(s);
}

unsigned tree_sitter_algol26_external_scanner_serialize(
    void *payload, char *buffer) {
  Scanner *s = payload;
  unsigned size = 0;

  // 1 byte: pending_dedents.
  buffer[size++] = (char)s->pending_dedents;
  // 1 byte: brace_depth.
  buffer[size++] = (char)s->brace_depth;

  // 1 byte: stack depth (capped to 255, which is generous — the
  // scanner would already have failed on deeper nesting).
  unsigned n = s->indents.size;
  if (n > 255) n = 255;
  buffer[size++] = (char)n;

  // 2 bytes per indent level (u16 — max indent 65535 columns,
  // which is far beyond any sane source file).
  for (unsigned i = 0; i < n; i++) {
    int v = s->indents.contents[i];
    buffer[size++] = (char)(v & 0xFF);
    buffer[size++] = (char)((v >> 8) & 0xFF);
  }
  return size;
}

void tree_sitter_algol26_external_scanner_deserialize(
    void *payload, const char *buffer, unsigned length) {
  Scanner *s = payload;
  array_clear(&s->indents);
  s->pending_dedents = 0;

  if (length == 0) {
    // Fresh scanner (first call or post-reset).
    array_push(&s->indents, 0);
    return;
  }

  unsigned i = 0;
  s->pending_dedents = (unsigned char)buffer[i++];
  s->brace_depth = (unsigned char)buffer[i++];
  unsigned n = (unsigned char)buffer[i++];
  for (unsigned j = 0; j < n && i + 1 < length; j++) {
    int lo = (unsigned char)buffer[i++];
    int hi = (unsigned char)buffer[i++];
    array_push(&s->indents, lo | (hi << 8));
  }
  if (s->indents.size == 0) array_push(&s->indents, 0);
}

bool tree_sitter_algol26_external_scanner_scan(
    void *payload, TSLexer *lexer, const bool *valid_symbols) {
  Scanner *s = payload;

  // ── Track brace depth (peek, don't consume) ──
  // ADR 0054. Brace-form bodies use `{ ... }`; indentation-form
  // bodies use INDENT/DEDENT. A `{` anywhere opens a brace scope,
  // during which the scanner must not emit INDENT/DEDENT. The
  // scanner peeks at `{`/`}` without advancing; returning false
  // lets the grammar's lexer consume the brace itself. The depth
  // update persists across scan() calls because it's serialized
  // into scanner state.
  //
  // This handles both block-start positions (`proc main {`) and
  // record-literal positions (`Point { x: 3 }`): any brace,
  // anywhere, increments depth; any closing brace decrements it.
  if (lexer->lookahead == '{') {
    s->brace_depth++;
    return false;
  }
  if (lexer->lookahead == '}') {
    if (s->brace_depth > 0) s->brace_depth--;
    return false;
  }

  // ── Emit any pending DEDENTs first ──
  if (s->pending_dedents > 0 && valid_symbols[DEDENT]) {
    s->pending_dedents--;
    if (s->indents.size > 1) array_pop(&s->indents);
    lexer->result_symbol = DEDENT;
    return true;
  }

  // Nothing to do unless INDENT or DEDENT is expected here.
  if (!valid_symbols[INDENT] && !valid_symbols[DEDENT]) return false;

  // Suppress INDENT/DEDENT inside a brace scope. A body that
  // started with `{` is brace-delimited; its internal newlines
  // and indentation are irrelevant.
  if (s->brace_depth > 0) return false;

  // Only trigger on a line boundary. Trailing spaces before the
  // newline are skipped as extras.
  if (lexer->lookahead != '\n' && lexer->lookahead != '\r') return false;

  // Consume newline(s) and any blank or comment-only lines.
  // These do not affect the indent stack.
  bool saw_newline = false;
  while (true) {
    if (lexer->lookahead == '\n' || lexer->lookahead == '\r') {
      lexer->advance(lexer, false);
      saw_newline = true;
      continue;
    }
    if (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
      // Peek past leading whitespace: if the line is blank or a
      // comment, skip it and keep looking for a real line.
      // We can't peek without advancing, so advance and remember
      // — the column counter below will restart if we hit another
      // newline.
      break;
    }
    break;
  }

  if (!saw_newline) return false;

  // Measure the indent of the line we landed on.
  int col = 0;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    col += (lexer->lookahead == '\t') ? 4 : 1;
    lexer->advance(lexer, true);
  }

  // Empty line or comment-only line: no indent change.
  if (lexer->lookahead == '\n' || lexer->lookahead == '\r' ||
      lexer->lookahead == 0) {
    return false;
  }
  if (lexer->lookahead == '/' /* possible // comment */) {
    // Don't consume the comment; just decline. The grammar's
    // `line_comment` extra will consume it, and the next scan()
    // call starts after it.
    return false;
  }

  int prev = s->indents.contents[s->indents.size - 1];

  if (col > prev && valid_symbols[INDENT]) {
    array_push(&s->indents, col);
    lexer->result_symbol = INDENT;
    return true;
  }

  if (col < prev) {
    // Pop levels until we land at or above the target.
    // Emit DEDENT (one per scan() call); extra pops are queued
    // in pending_dedents.
    array_pop(&s->indents);
    while (s->indents.size > 1 &&
           s->indents.contents[s->indents.size - 1] > col) {
      s->pending_dedents++;
      array_pop(&s->indents);
    }
    // If the line's indent doesn't exactly match any level on the
    // stack, the program is malformed. Returning false lets the
    // parser report a syntax error at this point rather than
    // looping.
    if (s->indents.contents[s->indents.size - 1] != col) {
      return false;
    }
    if (valid_symbols[DEDENT]) {
      lexer->result_symbol = DEDENT;
      return true;
    }
    return false;
  }

  // Same level: no INDENT/DEDENT. Return false; the parser
  // continues with the next grammar rule.
  return false;
}

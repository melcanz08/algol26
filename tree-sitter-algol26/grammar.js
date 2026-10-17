/**
 * @file ALGOL26 grammar for Tree-sitter
 * @author Rommel Caneos <rec0608m@gmail.com>
 * @license MIT
 *
 * Pass 1: brace-only. No external scanner, no indentation handling.
 * Parses examples/braces/*.gol. Indentation support (INDENT/DEDENT)
 * is added in a later pass once the brace grammar is clean.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "algol26",

  extras: $ => [/\s/, $.line_comment],

  word: $ => $.identifier,

  externals: $ => [
    $.indent,
    $.dedent,
  ],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.function_definition,
      $.procedure_definition,
      $.record_declaration,
      $.impl_declaration,
      $.union_declaration,
      $.enum_declaration,
    ),

    // ---- declarations ---------------------------------------------------

    function_definition: $ => seq(
      'fn', $.identifier,
      optional($.parameter_list),
      optional(seq('->', $.type)),
      $.block,
    ),

    procedure_definition: $ => seq(
      'proc', $.identifier,
      optional($.parameter_list),
      optional(seq('->', $.type)),
      $.block,
    ),

    parameter_list: $ => seq(
      '(', optional(seq($.parameter, repeat(seq(',', $.parameter)))), ')',
    ),

    parameter: $ => seq($.identifier, ':', $.type),

    record_declaration: $ => seq(
      'rec', $.identifier, $.field_list,
    ),

    field_list: $ => seq(
      '{', repeat($.field), '}',
    ),

    field: $ => seq($.identifier, ':', $.type),

    impl_declaration: $ => seq(
      'impl', $.identifier, '{',
        repeat(choice($.function_definition, $.procedure_definition)),
      '}',
    ),

    // Sum types (ADR 0055). Included here so union programs parse;
    // highlighted later, lowered only by the interpreter for now.
    union_declaration: $ => seq(
      'union', $.identifier, '{',
        repeat($.union_variant),
      '}',
    ),

    union_variant: $ => seq(
      $.identifier,
      optional(seq('(', optional(seq($.type, repeat(seq(',', $.type)))), ')')),
    ),

    // Enum declarations (ADR 0055): a union where variants have no payloads.
    // Treated as a distinct node so highlights can color them differently.
    enum_declaration: $ => seq(
      'enum', $.identifier, '{',
        repeat($.enum_variant),
      '}',
    ),

    enum_variant: $ => $.identifier,

    // ---- statements -----------------------------------------------------

    // ADR 0054. Two block forms, semantically identical:
    //   brace form:  { stmt stmt ... }
    //   indentation form:  INDENT stmt stmt ... DEDENT
    // The external scanner emits INDENT/DEDENT; brace blocks use
    // the literal '{' '}' tokens. A block is one or the other.
    block: $ => choice(
      seq('{', repeat($._statement), '}'),
      seq($.indent, repeat($._statement), $.dedent),
    ),

    _statement: $ => choice(
      $.variable_declaration,
      $.assignment,
      $.expression_statement,
      $.if_expression,
      $.for_statement,
      $.while_statement,
      $.match_expression,
      $.return_statement,
      $.defer_statement,
      $.break_statement,
      $.continue_statement,
      // A bare `block` is not a statement. Blocks are always the
      // body of a control-flow construct, function, or procedure
      // header, so `_statement` should not accept them directly.
      // Including `$.block` here caused the parser, when inside
      // a brace body (`{ expr <newline>`), to attempt to open a
      // nested indent block at the next statement, emitting a
      // phantom INDENT. See ADR 0054.
    ),

    variable_declaration: $ => seq(
      choice('val', 'var'), $.identifier,
      optional(seq(':', $.type)),
      ':=',
      $._arg_expression,
    ),

    assignment: $ => seq(
      $._expression, ':=', $._expression,
    ),

    expression_statement: $ => $._expression,

    return_statement: $ => prec.right(seq('return', optional($._arg_expression))),

    defer_statement: $ => seq('defer', $._statement),

    break_statement: $ => 'break',
    continue_statement: $ => 'continue',

    for_statement: $ => seq(
      'for', $.identifier, 'in', $._expression, $.block,
    ),

    while_statement: $ => seq(
      'while', $._expression, $.block,
    ),

    // `if` is an expression in this language (used in function bodies).
    if_expression: $ => prec.right(seq(
      'if', $._expression, $.block,
      optional(seq('else', choice($.if_expression, $.block))),
    )),

    match_expression: $ => seq(
      'match', $._expression,
      '{', repeat($.match_arm), '}',
    ),

    match_arm: $ => seq(
      'case', $._pattern,
      optional(seq('if', $._expression)),   // guard
      'then', $.block,
    ),

    _pattern: $ => choice(
      $.constructor_pattern,
      $.identifier,
      $.literal,
    ),

    constructor_pattern: $ => prec(1, seq(
      $.identifier, '(',
        optional(seq($.identifier, repeat(seq(',', $.identifier)))),
      ')',
    )),

    // ---- expressions ----------------------------------------------------

    _expression: $ => choice(
      $.literal,
      $.identifier,
      $.primitive_type,
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.method_call_expression,
      $.field_expression,
      $.index_expression,
      $.list_literal,
      $.paren_expression,
      $.deref_expression,
      $.borrow_expression,
    ),

    // Argument-position expression: like _expression, but also allows
    // record literals and map literals. Used in call/constructor
    // argument lists and variable-declaration RHS, where a `{` cannot
    // start a block.
    _arg_expression: $ => choice(
      $._expression,
      $.record_literal,
      $.map_literal,
    ),

    paren_expression: $ => seq('(', $._expression, ')'),

    binary_expression: $ => {
      const table = [
        ['||', 1],
        ['&&', 2],
        ['==', 3], ['=', 3], ['!=', 3], ['<', 3], ['<=', 3], ['>', 3], ['>=', 3],
        ['+', 4], ['-', 4],
        ['*', 5], ['/', 5], ['%', 5],
      ];
      return choice(...table.map(([op, p]) =>
        prec.left(p, seq(
          field('left', $._expression),
          field('op', op),
          field('right', $._expression),
        )),
      ));
    },

    unary_expression: $ => prec(6, seq(
      choice('-', '!', 'not'),
      $._expression,
    )),

    deref_expression: $ => prec(7, seq('*', $._expression)),

    borrow_expression: $ => prec(7, seq('&', optional('mut'), $._expression)),

    call_expression: $ => prec(8, seq(
      field('function', choice($.identifier, $.field_expression)),
      '(', optional(seq($._arg_expression, repeat(seq(',', $._arg_expression)))), ')',
    )),

    method_call_expression: $ => prec(8, seq(
      field('receiver', $._expression),
      '.', field('method', $.identifier),
      '(', optional(seq($._arg_expression, repeat(seq(',', $._arg_expression)))), ')',
    )),

    field_expression: $ => prec(9, seq(
      field('object', $._expression),
      '.', field('field', $.identifier),
    )),

    index_expression: $ => prec(9, seq(
      field('object', $._expression),
      '[', $._expression, ']',
    )),

    list_literal: $ => seq(
      '[', optional(seq($._arg_expression, repeat(seq(',', $._arg_expression)))), ']',
    ),

    // Map {} / Map<K, V> {} — map literal.
    map_literal: $ => prec(8, seq(
      'Map',
      optional(seq('<', $.type, repeat(seq(',', $.type)), '>')),
      '{',
        optional(seq($.map_entry, repeat(seq(',', $.map_entry)))),
      '}',
    )),

    map_entry: $ => seq($._arg_expression, ':', $._arg_expression),

    record_literal: $ => prec(8, seq(
      $.identifier, '{',
        optional(seq($.field_init, repeat(seq(',', $.field_init)))),
      '}',
    )),

    field_init: $ => seq($.identifier, ':', $._arg_expression),

    // ---- types ----------------------------------------------------------

    type: $ => choice(
      $.primitive_type,
      $.identifier,
      $.generic_type,
      $.reference_type,
      $.optional_type,
    ),

    primitive_type: $ => choice('Int', 'Float', 'Bool', 'String'),

    generic_type: $ => seq(
      $.identifier, '<', $.type, repeat(seq(',', $.type)), '>',
    ),

    reference_type: $ => seq('&', optional('mut'), $.type),
    optional_type: $ => seq('?', $.type),

    // ---- terminals ------------------------------------------------------

    literal: $ => choice(
      $.integer,
      $.float,
      $.string,
      $.boolean,
    ),

    integer: $ => /-?[0-9]+/,
    float:   $ => /-?[0-9]+\.[0-9]+/,
    boolean: $ => choice('true', 'false'),
    string:  $ => seq(
      '"',
      repeat(choice(
        /[^"\\]/,
        /\\./,
      )),
      '"',
    ),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    line_comment: $ => token(seq('//', /.*/)),
  },
});

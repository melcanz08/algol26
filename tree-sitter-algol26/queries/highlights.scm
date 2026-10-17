; ALGOL26 syntax highlighting queries.
; Capture names follow Tree-sitter conventions:
;   https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting
;
; IMPORTANT: only tokens that actually exist in grammar.js can appear
; in a `[...]` set. Adding a keyword here before the grammar has a rule
; that uses it produces "Invalid node type" at query-compile time.

; ---- Comments ----
(line_comment) @comment

; ---- Keywords (declarations) ----
[
  "fn" "proc" "rec" "impl" "enum" "union"
  "val" "var"
] @keyword

; ---- Keywords (control flow) ----
[
  "if" "else" "match" "case" "then" "for" "in" "while"
  "return" "defer"
] @keyword.control

; ---- Type names (declaration sites) ----
(record_declaration (identifier) @type)
(enum_declaration (identifier) @type)
(union_declaration (identifier) @type)
(impl_declaration (identifier) @type)

; ---- Primitive types ----
(primitive_type) @type.builtin

; ---- Function / procedure names (declaration sites) ----
(function_definition (identifier) @function)
(procedure_definition (identifier) @function)

; ---- Function call callees ----
(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (field_expression
    field: (identifier) @function.method.call))

; ---- Field access: the field name, not the receiver ----
(field_expression
  field: (identifier) @property)

; ---- Record-literal and record-declaration field names ----
(field_init (identifier) @property)
(field (identifier) @property)

; ---- Enum / union variants ----
(enum_variant (identifier) @constant)
(union_variant (identifier) @constructor)
(constructor_pattern (identifier) @constructor)

; ---- Parameters ----
(parameter (identifier) @variable.parameter)

; ---- Variable declarations ----
(variable_declaration (identifier) @variable)

; ---- Booleans ----
(boolean) @constant.builtin

; ---- Numbers ----
(integer) @number
(float) @number.float

; ---- Strings ----
(string) @string

; ---- Operators ----
[
  "+" "-" "*" "/" "%"
  "==" "=" "!=" "<" "<=" ">" ">="
  "&&" "||" "!"
  ":="
  "&"
] @operator

; ---- Punctuation ----
[ "(" ")" "[" "]" "{" "}" ] @punctuation.bracket
[ "," ":" "." ] @punctuation.delimiter

; ---- Type annotations: inner identifiers of reference/generic types ----
(reference_type (type (identifier) @type))
(generic_type (identifier) @type)

; ---- `self` ----
((identifier) @variable.builtin
 (#eq? @variable.builtin "self"))

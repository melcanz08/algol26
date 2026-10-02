#!/usr/bin/env python3
"""A3 fixes: import EnumTypeId in items.rs, thread &ast.enums in
type_check.rs."""

from pathlib import Path

ITEMS = Path("src/semantics/analyzer/items.rs")
TYPE_CHECK = Path("src/compiler/passes/type_check.rs")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


patch(
    ITEMS,
    [
        (
            """use super::*;
use crate::frontend::ast::{RecordDecl, TypeSyntax};""",
            """use super::*;
use crate::common::types::EnumTypeId;
use crate::frontend::ast::{RecordDecl, TypeSyntax};""",
        ),
    ],
)

patch(
    TYPE_CHECK,
    [
        (
            """        match crate::compiler::type_check_program(
            &ast.functions,
            &ast.traits,
            &ast.impls,
            &ast.records,
            &ast.distincts,
        ) {""",
            """        match crate::compiler::type_check_program(
            &ast.functions,
            &ast.traits,
            &ast.impls,
            &ast.records,
            &ast.distincts,
            &ast.enums,
        ) {""",
        ),
    ],
)

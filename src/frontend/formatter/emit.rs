// src/frontend/formatter/emit.rs
//
// ADR 0058. Canonical source emitter.
//
// The emitter walks the AST and produces source-shaped text. It
// knows nothing about comments or trivia; session 2b layers
// comment interleaving on top by threading a comment cursor
// through the emission.

use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprKind, FunctionDecl, ImplBlock, Program, RecordDecl,
    Stmt, SubrangeDecl, TraitDecl, TypeSyntax, UnaryOp,
};
use crate::frontend::lexer::Comment;

/// The canonical indentation unit. ADR 0058 specifies 4 spaces.
const INDENT: &str = "    ";

/// ADR 0058. The block form a file uses. Decided once per file,
/// from the first block-bearing construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockForm {
    /// Indentation-delimited blocks (ADR 0001). The canonical form.
    Indentation,
    /// Brace-delimited blocks (ADR 0054).
    Braces,
}

/// The formatter's state. Holds the block form, the current
/// indentation level, and the output buffer.
pub struct Formatter {
    block_form: BlockForm,
    output: String,
    /// ADR 0058 session 2b. Comments from the lexer, in source order.
    /// Emitted when the formatter passes their source position.
    comments: Vec<Comment>,
    /// Index of the next comment to emit.
    comment_cursor: usize,
}

impl Formatter {
    /// Construct a formatter for a program, reading the block form
    /// from the source. The source is the original text; the
    /// program is the parsed AST; `comments` are the lexer's
    /// retained comments (ADR 0058 session 1).
    pub fn new(_program: &Program, source: &str, comments: Vec<Comment>) -> Self {
        let block_form = detect_block_form(source);
        Formatter {
            block_form,
            output: String::new(),
            comments,
            comment_cursor: 0,
        }
    }

    /// Emit the program in canonical form.
    pub fn format(mut self, program: &Program) -> String {
        self.emit_program(program);
        // ADR 0058 session 2b. Emit any trailing comments (comments
        // after the last AST node) before finalizing.
        self.flush_comments();
        // Exactly one trailing newline.
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output
    }

    fn emit_program(&mut self, program: &Program) {
        // Imports first, then each top-level item in source order.
        // The ADR says "no reordering" — so we follow the AST's
        // order: functions, then traits, then impls, then records,
        // etc. Preserving source order across these categories
        // would require the parser to record it; the AST does not,
        // so we emit category by category.
        let mut first = true;

        for imp in &program.imports {
            if !first {
                self.output.push('\n');
            }
            self.emit_line(0, &format!("import {}", imp));
            first = false;
        }

        for f in &program.functions {
            if !first {
                self.output.push('\n');
            }
            self.emit_function(0, f);
            first = false;
        }
        for t in &program.traits {
            if !first {
                self.output.push('\n');
            }
            self.emit_trait(0, t);
            first = false;
        }
        for im in &program.impls {
            if !first {
                self.output.push('\n');
            }
            self.emit_impl(0, im);
            first = false;
        }
        for r in &program.records {
            if !first {
                self.output.push('\n');
            }
            self.emit_record(0, r);
            first = false;
        }
        for e in &program.enum_decls {
            if !first {
                self.output.push('\n');
            }
            self.emit_enum(0, e);
            first = false;
        }
        for d in &program.distinct_decls {
            if !first {
                self.output.push('\n');
            }
            self.emit_distinct(0, d);
            first = false;
        }
        for s in &program.subrange_decls {
            if !first {
                self.output.push('\n');
            }
            self.emit_subrange(0, s);
            first = false;
        }
    }

    // ─── Helpers ───

    /// ADR 0058 session 2b. Emit any pending comments whose source
    /// position precedes (`line`, `col`) at `level`.
    ///
    /// Called before emitting each AST node. Comments are emitted
    /// in source order; `comment_cursor` advances monotonically.
    /// Standalone comments go on their own line at `level`;
    /// trailing comments attach to the last emitted line.
    fn drain_comments_before(&mut self, line: usize, col: usize, level: usize) {
        while self.comment_cursor < self.comments.len() {
            let c = self.comments[self.comment_cursor].clone();
            if (c.line, c.start_col) >= (line, col) {
                break;
            }
            self.emit_comment(&c, level);
            self.comment_cursor += 1;
        }
    }

    /// Emit a single comment at the current position.
    fn emit_comment(&mut self, c: &Comment, level: usize) {
        if c.standalone {
            // Own-line comment. Emit at `level` indentation.
            self.indent(level);
            self.output.push_str(&c.text);
            self.output.push('\n');
        } else {
            // Trailing comment. Attach to the previous line, or
            // emit standalone if the previous output didn't end with
            // a newline (defensive).
            if self.output.ends_with('\n') {
                self.output.pop();
                self.output.push(' ');
                self.output.push_str(&c.text);
                self.output.push('\n');
            } else {
                self.output.push(' ');
                self.output.push_str(&c.text);
                self.output.push('\n');
            }
        }
    }

    /// ADR 0058 session 2b. Flush any remaining comments (trailing
    /// comments at end of file).
    fn flush_comments(&mut self) {
        while self.comment_cursor < self.comments.len() {
            let c = self.comments[self.comment_cursor].clone();
            self.emit_comment(&c, 0);
            self.comment_cursor += 1;
        }
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.output.push_str(INDENT);
        }
    }

    fn emit_line(&mut self, level: usize, text: &str) {
        self.indent(level);
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn emit_block_header(&mut self, level: usize, header: &str) {
        self.indent(level);
        self.output.push_str(header);
        match self.block_form {
            BlockForm::Braces => self.output.push_str(" {\n"),
            BlockForm::Indentation => self.output.push('\n'),
        }
    }

    fn emit_block_footer(&mut self, level: usize) {
        if self.block_form == BlockForm::Braces {
            self.indent(level);
            self.output.push_str("}\n");
        }
    }

    /// Emit a nested block. `body` is called with the inner level.
    fn emit_block<F: FnOnce(&mut Self, usize)>(&mut self, level: usize, body: F) {
        body(self, level + 1);
        self.emit_block_footer(level);
    }

    // ─── Top-level items ───

    fn emit_function(&mut self, level: usize, func: &FunctionDecl) {
        // ADR 0058 session 2b. Drain any comments whose source
        // position precedes the function declaration. The span is
        // the `proc`/`fn` keyword's position (populated by the
        // parser); comments on lines before it are emitted before
        // the function header.
        self.drain_comments_before(func.span.start_line, func.span.start_column, level);
        // extern declarations have no body.
        if func.is_extern {
            let mut header = String::from("extern ");
            if let Some(info) = &func.ffi_info {
                if let Some(abi) = &info.abi {
                    header.push('"');
                    header.push_str(abi);
                    header.push_str("\" ");
                }
            }
            let kind = if func.receiver.is_some() { "fn" } else { "fn" };
            header.push_str(kind);
            header.push(' ');
            header.push_str(&func.name);
            self.emit_type_params(&mut header, &func.type_params);
            header.push('(');
            self.emit_params(&mut header, &func.params, func);
            header.push(')');
            if let Some(rt) = &func.return_type {
                header.push_str(" -> ");
                header.push_str(&type_str(rt));
            }
            self.emit_line(level, &header);
            return;
        }

        // ADR 0048. `fn` vs `proc` is a source-level distinction
        // the parser records on the AST (`is_fn`). Emitting `fn`
        // for a `proc` with a return type would produce a body the
        // parser rejects, since `fn` bodies may not contain
        // statements or `return`.
        let kind = if func.is_fn { "fn" } else { "proc" };

        let mut header = String::new();
        header.push_str(kind);
        header.push(' ');
        header.push_str(&func.name);
        self.emit_type_params(&mut header, &func.type_params);
        header.push('(');
        self.emit_params(&mut header, &func.params, func);
        header.push(')');
        if let Some(rt) = &func.return_type {
            header.push_str(" -> ");
            header.push_str(&type_str(rt));
        }
        if !func.where_clauses.is_empty() {
            header.push_str(" where ");
            let clauses: Vec<String> = func
                .where_clauses
                .iter()
                .map(|wc| format!("{}: {}", wc.type_param, wc.trait_name))
                .collect();
            header.push_str(&clauses.join(", "));
        }

        self.emit_block_header(level, &header);

        // ADR 0048. `fn` and `proc` have structurally different
        // bodies:
        //
        //   - `fn`   — a single expression. The parser stores it as
        //              `[Stmt::Return { value: Some(e), span }]`,
        //              but the `return` keyword is *forbidden* in
        //              source. Emitting `Stmt::Return` through the
        //              generic statement path would produce
        //              `return 42.0` inside a `fn` body, which the
        //              parser rejects.
        //
        //   - `proc`  — a statement sequence. `return E` is
        //              permitted and emits as `return E`.
        //
        // Two separate paths so the `fn` body never goes through
        // `emit_stmt`.
        if kind == "fn" {
            self.emit_block(level, |this, inner| {
                this.emit_fn_body(inner, &func.body);
            });
        } else {
            self.emit_block(level, |this, inner| {
                for s in &func.body {
                    this.emit_stmt(inner, s);
                }
            });
        }
    }

    /// ADR 0048. Emit a `fn` body. A well-formed `fn` body is a
    /// single expression, which the parser wraps as
    /// `[Stmt::Return { value: Some(e), span }]`.
    ///
    /// The `return` keyword is dropped; the expression itself is
    /// the body. An `fn` whose expression is itself a control-flow
    /// construct (`if`, `match`) is emitted by `expr_inline`, which
    /// renders those on a single line.
    fn emit_fn_body(&mut self, level: usize, body: &[Stmt]) {
        match body {
            [Stmt::Return { value: Some(e), .. }] => {
                let line = expr_inline(e);
                self.emit_line(level, &line);
            }
            [] => {
                // An empty `fn` body is a parse error upstream.
                // Defensive: emit nothing.
            }
            _ => {
                // Defensive fallback. A well-formed `fn` body is
                // always the single-return shape above. If we get
                // here, the AST is malformed; emit as statements
                // so the formatter doesn't silently drop content.
                for s in body {
                    self.emit_stmt(level, s);
                }
            }
        }
    }

    fn emit_type_params(&self, out: &mut String, params: &[String]) {
        if params.is_empty() {
            return;
        }
        out.push('<');
        out.push_str(&params.join(", "));
        out.push('>');
    }

    fn emit_params(
        &self,
        out: &mut String,
        params: &[(String, Option<TypeSyntax>)],
        _func: &FunctionDecl,
    ) {
        let mut parts = Vec::new();
        for (name, ty) in params {
            let mut p = name.clone();
            if let Some(t) = ty {
                p.push_str(": ");
                p.push_str(&type_str(t));
            }
            parts.push(p);
        }
        out.push_str(&parts.join(", "));
    }

    fn emit_trait(&mut self, level: usize, tr: &TraitDecl) {
        let mut header = String::from("trait ");
        header.push_str(&tr.name);
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            for m in &tr.methods {
                let mut mheader = String::from("fn ");
                mheader.push_str(&m.name);
                mheader.push('(');
                let parts: Vec<String> = m
                    .params
                    .iter()
                    .map(|(n, ty)| match ty {
                        Some(t) => format!("{}: {}", n, type_str(t)),
                        None => n.clone(),
                    })
                    .collect();
                mheader.push_str(&parts.join(", "));
                mheader.push(')');
                if let Some(rt) = &m.return_type {
                    mheader.push_str(" -> ");
                    mheader.push_str(&type_str(rt));
                }
                this.emit_line(inner, &mheader);
            }
        });
    }

    fn emit_impl(&mut self, level: usize, im: &ImplBlock) {
        let mut header = String::from("impl");
        if !im.type_params.is_empty() {
            header.push('<');
            header.push_str(&im.type_params.join(", "));
            header.push('>');
        }
        header.push(' ');
        if let Some(tn) = &im.trait_name {
            header.push_str(tn);
            header.push_str(" for ");
        }
        header.push_str(&im.target_type);
        if !im.target_type_args.is_empty() {
            header.push('<');
            let args: Vec<String> = im.target_type_args.iter().map(type_str).collect();
            header.push_str(&args.join(", "));
            header.push('>');
        }
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            for m in &im.methods {
                this.emit_function(inner, m);
            }
        });
    }

    fn emit_record(&mut self, level: usize, rec: &RecordDecl) {
        let mut header = String::from("rec ");
        header.push_str(&rec.name);
        if !rec.type_params.is_empty() {
            header.push('<');
            header.push_str(&rec.type_params.join(", "));
            header.push('>');
        }
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            for (name, ty, _vis) in &rec.fields {
                this.emit_line(inner, &format!("{}: {}", name, type_str(ty)));
            }
        });
    }

    fn emit_enum(&mut self, level: usize, e: &EnumDecl) {
        let mut header = String::from("enum ");
        header.push_str(&e.name);
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            for v in &e.variants {
                this.emit_line(inner, v);
            }
        });
    }

    fn emit_distinct(&mut self, level: usize, d: &DistinctDecl) {
        let text = format!("type {} distinct {}", d.name, type_str(&d.base));
        self.emit_line(level, &text);
    }

    fn emit_subrange(&mut self, level: usize, s: &SubrangeDecl) {
        let text = format!(
            "type {} {} in {}..{}",
            s.name,
            type_str(&s.base),
            expr_inline(&s.low),
            expr_inline(&s.high)
        );
        self.emit_line(level, &text);
    }

    // ─── Statements ───

    // ─── Expression emission (ADR 0058 session 4a) ───
    //
    // `emit_expr` writes an expression to `self.output` at a given
    // level. Block-shaped expressions (`if`, `match`, `try`,
    // `for`, `while`, `Block`) emit multi-line using the same
    // block-form-aware helpers as statement emission. Other
    // expressions delegate to `expr_inline`, which returns a
    // single-line string.

    /// True if `expr` is block-shaped and cannot be rendered on a
    /// single line. Used by callers that need to know which emitter
    /// to invoke.
    fn is_block_shaped(expr: &Expr) -> bool {
        matches!(
            expr.kind,
            ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::TryCatch { .. }
                | ExprKind::Block { .. }
                | ExprKind::For { .. }
                | ExprKind::While { .. }
        )
    }

    /// Emit `expr` at `level`. The first line of the emitted text
    /// begins at `level`'s indentation; block-shaped expressions
    /// continue at deeper levels.
    fn emit_expr(&mut self, level: usize, expr: &Expr) {
        match &expr.kind {
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.emit_if(level, condition, then_branch, else_branch.as_deref());
            }
            ExprKind::Match { value, cases, .. } => {
                self.emit_match(level, value, cases);
            }
            ExprKind::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
                ..
            } => {
                self.emit_try_catch(
                    level,
                    try_branch,
                    catch_var.as_deref(),
                    catch_branch,
                    finally_body.as_deref(),
                );
            }
            ExprKind::For { .. } | ExprKind::While { .. } => {
                // Statement-shaped loops already have an emitter;
                // reuse it.
                self.emit_expr_stmt(level, expr);
            }
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.emit_stmt(level, s);
                }
                if let Some(e) = trailing_expr {
                    self.emit_expr(level, e);
                }
            }
            _ => {
                // Single-line expression.
                let text = expr_inline(expr);
                self.emit_line(level, &text);
            }
        }
    }

    /// Emit an `if` expression at `level`.
    fn emit_if(
        &mut self,
        level: usize,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
    ) {
        let mut header = format!("if {}", expr_inline(condition));
        if self.block_form == BlockForm::Indentation {
            header.push_str(" then");
        }
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            this.emit_expr(inner, then_branch);
        });
        if let Some(eb) = else_branch {
            self.emit_else(level, eb);
        }
    }

    /// Emit a `match` expression at `level`.
    fn emit_match(
        &mut self,
        level: usize,
        value: &Expr,
        cases: &[crate::frontend::ast::MatchCaseExpr],
    ) {
        let header = format!("match {}", expr_inline(value));
        self.emit_block_header(level, &header);
        self.emit_block(level, |this, inner| {
            for case in cases {
                let pat_text = pattern_text(&case.pattern);
                // Brace form requires `then` between pattern and
                // body (ADR 0054); indentation form does not.
                let case_header = if this.block_form == BlockForm::Braces {
                    format!("case {} then", pat_text)
                } else {
                    format!("case {}", pat_text)
                };
                this.emit_block_header(inner, &case_header);
                this.emit_block(inner, |this2, deeper| {
                    this2.emit_expr(deeper, &case.body);
                });
            }
        });
    }

    /// Emit a `try`/`catch`/`finally` expression at `level`, with
    /// the `try` keyword on its own line.
    fn emit_try_catch(
        &mut self,
        level: usize,
        try_branch: &Expr,
        catch_var: Option<&str>,
        catch_branch: &Expr,
        finally_body: Option<&[Stmt]>,
    ) {
        self.emit_block_header(level, "try");
        self.emit_try_catch_body(level, try_branch, catch_var, catch_branch, finally_body);
    }

    /// ADR 0058 session 4b. Emit the body and clauses of a
    /// try/catch/finally, assuming the `try` keyword has already
    /// been emitted at the end of the previous line. Used by
    /// `VarDecl` to inline `try` with the `:=` header:
    ///
    ///     val r1: Int := try
    ///         might_fail(true)
    ///     catch err
    ///         ...
    ///
    /// The caller emits `val r1: Int := try` at `level`, then calls
    /// this method with `level`. The try body indents one deeper;
    /// `catch` sits at `level`.
    fn emit_try_catch_body(
        &mut self,
        level: usize,
        try_branch: &Expr,
        catch_var: Option<&str>,
        catch_branch: &Expr,
        finally_body: Option<&[Stmt]>,
    ) {
        self.emit_block(level, |this, inner| {
            this.emit_expr(inner, try_branch);
        });
        let catch_header = match catch_var {
            Some(name) => format!("catch {}", name),
            None => "catch".to_string(),
        };
        self.emit_block_header(level, &catch_header);
        self.emit_block(level, |this, inner| {
            this.emit_expr(inner, catch_branch);
        });
        if let Some(body) = finally_body {
            self.emit_block_header(level, "finally");
            self.emit_block(level, |this, inner| {
                for s in body {
                    this.emit_stmt(inner, s);
                }
            });
        }
    }

    fn emit_stmt(&mut self, level: usize, stmt: &Stmt) {
        // ADR 0058 session 2b. Drain any comments whose source
        // position precedes this statement.
        let span = stmt.span();
        self.drain_comments_before(span.start_line, span.start_column, level);

        match stmt {
            Stmt::VarDecl {
                name,
                value,
                type_annotation,
                mutable,
                ..
            } => {
                let mut line = String::new();
                line.push_str(if *mutable { "var " } else { "val " });
                line.push_str(name);
                if let Some(t) = type_annotation {
                    line.push_str(": ");
                    line.push_str(&type_str(t));
                }
                line.push_str(" :=");

                // ADR 0058 session 4a. If the value is block-shaped
                // (e.g. `val x := if ...`), emit the header and then
                // the block at one deeper level.
                if Self::is_block_shaped(value) {
                    // ADR 0058 session 4b. A `try` initializer is
                    // special: the parser expects `try` inline with
                    // `:=`. Emit `val NAME := try` on the header
                    // line, then the try body and clauses at `level`.
                    if let ExprKind::TryCatch {
                        try_branch,
                        catch_var,
                        catch_branch,
                        finally_body,
                        ..
                    } = &value.kind
                    {
                        line.push_str(" try");
                        self.emit_line(level, &line);
                        self.emit_try_catch_body(
                            level,
                            try_branch,
                            catch_var.as_deref(),
                            catch_branch,
                            finally_body.as_deref(),
                        );
                    } else {
                        // `if`, `match`, etc.: keyword on its own
                        // line, one indent deeper.
                        self.emit_line(level, &line);
                        self.emit_expr(level + 1, value);
                    }
                } else {
                    line.push(' ');
                    line.push_str(&expr_inline(value));
                    self.emit_line(level, &line);
                }
            }
            Stmt::Assign { name, value, .. } => {
                let line = format!("{} := {}", name, expr_inline(value));
                self.emit_line(level, &line);
            }
            Stmt::ArrayAssign {
                array,
                index,
                value,
                ..
            } => {
                let line = format!(
                    "{}[{}] := {}",
                    array,
                    expr_inline(index),
                    expr_inline(value)
                );
                self.emit_line(level, &line);
            }
            Stmt::FieldAssign {
                target,
                field,
                value,
                ..
            } => {
                let line = format!("{}.{} := {}", target, field, expr_inline(value));
                self.emit_line(level, &line);
            }
            Stmt::DerefAssign { target, value, .. } => {
                let line = format!("{} := {}", expr_inline(target), expr_inline(value));
                self.emit_line(level, &line);
            }
            Stmt::Return { value, .. } => match value {
                Some(e) => self.emit_line(level, &format!("return {}", expr_inline(e))),
                None => self.emit_line(level, "return"),
            },
            Stmt::Print { expr, .. } => {
                self.emit_line(level, &format!("print({})", expr_inline(expr)));
            }
            Stmt::Break(_) => self.emit_line(level, "break"),
            Stmt::Continue(_) => self.emit_line(level, "continue"),
            Stmt::Defer { stmt, .. } => {
                self.emit_line(level, "defer");
                self.emit_stmt(level + 1, stmt);
            }
            Stmt::Expression(expr) => {
                self.emit_expr_stmt(level, expr);
            }
            Stmt::RegionBlock { name, body, .. } => {
                self.emit_block_header(level, &format!("region {}", name));
                self.emit_block(level, |this, inner| {
                    for s in body {
                        this.emit_stmt(inner, s);
                    }
                });
            }
            Stmt::UnsafeBlock { body, .. } => {
                self.emit_block_header(level, "unsafe");
                self.emit_block(level, |this, inner| {
                    for s in body {
                        this.emit_stmt(inner, s);
                    }
                });
            }
            Stmt::Import { path, .. } => {
                self.emit_line(level, &format!("import {}", path));
            }
            Stmt::Spawn { body, .. } => {
                self.emit_block_header(level, "spawn");
                self.emit_block(level, |this, inner| {
                    for s in body {
                        this.emit_stmt(inner, s);
                    }
                });
            }
            Stmt::Parallel { blocks, .. } => {
                // Parallel blocks are separated by `and`.
                for (i, block) in blocks.iter().enumerate() {
                    if i == 0 {
                        self.emit_block_header(level, "parallel do");
                    } else {
                        self.emit_block_header(level, "and");
                    }
                    self.emit_block(level, |this, inner| {
                        for s in block {
                            this.emit_stmt(inner, s);
                        }
                    });
                }
            }
            Stmt::ChannelDecl { name, .. } => {
                self.emit_line(level, &format!("channel {}", name));
            }
            Stmt::Send { channel, value, .. } => {
                self.emit_line(
                    level,
                    &format!("send {} <- {}", channel, expr_inline(value)),
                );
            }
            Stmt::Receive {
                channel, target, ..
            } => {
                if target.is_empty() {
                    self.emit_line(level, &format!("receive {}", channel));
                } else {
                    self.emit_line(level, &format!("receive {} -> {}", channel, target));
                }
            }
        }
    }

    fn emit_expr_stmt(&mut self, level: usize, expr: &Expr) {
        match &expr.kind {
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.emit_stmt(level, s);
                }
                if let Some(e) = trailing_expr {
                    let line = expr_inline(e);
                    self.emit_line(level, &line);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let header = format!("if {}", expr_inline(condition));
                let header = if self.block_form == BlockForm::Indentation {
                    format!("{} then", header)
                } else {
                    header
                };
                self.emit_block_header(level, &header);
                self.emit_block(level, |this, inner| {
                    this.emit_expr_block(inner, then_branch);
                });
                if let Some(eb) = else_branch {
                    self.emit_else(level, eb);
                }
            }
            ExprKind::For {
                var,
                iterable,
                body,
                trailing_expr,
                ..
            } => {
                let header = format!("for {} in {}", var, expr_inline(iterable));
                self.emit_block_header(level, &header);
                self.emit_block(level, |this, inner| {
                    for s in body {
                        this.emit_stmt(inner, s);
                    }
                    if let Some(e) = trailing_expr {
                        this.emit_line(inner, &expr_inline(e));
                    }
                });
            }
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                ..
            } => {
                let header = format!("while {}", expr_inline(condition));
                self.emit_block_header(level, &header);
                self.emit_block(level, |this, inner| {
                    for s in body {
                        this.emit_stmt(inner, s);
                    }
                    if let Some(e) = trailing_expr {
                        this.emit_line(inner, &expr_inline(e));
                    }
                });
            }
            _ => {
                let line = expr_inline(expr);
                self.emit_line(level, &line);
            }
        }
    }

    fn emit_expr_block(&mut self, level: usize, expr: &Expr) {
        match &expr.kind {
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.emit_stmt(level, s);
                }
                if let Some(e) = trailing_expr {
                    self.emit_line(level, &expr_inline(e));
                }
            }
            _ => {
                self.emit_line(level, &expr_inline(expr));
            }
        }
    }

    fn emit_else(&mut self, level: usize, expr: &Expr) {
        // `else if` chains: the `else` arm is a Block wrapping an If.
        // Emit `else if` on one line when possible.
        match &expr.kind {
            ExprKind::Block {
                statements,
                trailing_expr: Some(inner),
                ..
            } if statements.is_empty() => {
                if let ExprKind::If { .. } = &inner.kind {
                    if self.block_form == BlockForm::Braces {
                        // `} else if cond {`
                        // The footer was already emitted, so we
                        // emit `else if ... {` continuing the line.
                        // For simplicity, emit `else` on the same
                        // line as the previous `}`.
                        // This requires tracking line position; we
                        // simplify by emitting `else` on its own
                        // line with braces.
                        self.emit_block_header(level, "else");
                        self.emit_block(level, |this, inner2| {
                            this.emit_expr_stmt(inner2, inner);
                        });
                        return;
                    } else {
                        self.indent(level);
                        self.output.push_str("else ");
                        self.emit_stmt(level, &Stmt::Expression((**inner).clone()));
                        return;
                    }
                }
                self.emit_block_header(level, "else");
                self.emit_block(level, |this, inner2| {
                    this.emit_expr_block(inner2, inner);
                });
            }
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                self.emit_block_header(level, "else");
                self.emit_block(level, |this, inner| {
                    for s in statements {
                        this.emit_stmt(inner, s);
                    }
                    if let Some(e) = trailing_expr {
                        this.emit_line(inner, &expr_inline(e));
                    }
                });
            }
            _ => {
                self.emit_block_header(level, "else");
                self.emit_block(level, |this, inner| {
                    this.emit_line(inner, &expr_inline(expr));
                });
            }
        }
    }
}

// ─── Free functions ───

/// ADR 0058. Detect the file's block form from the first
/// block-bearing construct. Scans the source text for the first
/// `proc`, `fn`, `if`, `for`, `while`, `match`, `impl`, `rec`,
/// `enum`, `trait`, `region`, `unsafe`, or `defer` keyword at the
/// start of a statement, then checks whether the next non-whitespace
/// token is `{`.
fn detect_block_form(source: &str) -> BlockForm {
    let keywords = [
        "proc", "fn", "if", "for", "while", "match", "impl", "rec", "enum", "trait", "region",
        "unsafe", "defer",
    ];
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        for kw in &keywords {
            if let Some(after) = trimmed.strip_prefix(kw) {
                // The keyword must be followed by whitespace or a
                // delimiter to be a keyword. Otherwise it's a
                // prefix of an identifier.
                let is_kw =
                    after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == '(');
                if !is_kw {
                    continue;
                }
                // Look ahead for `{` anywhere on this line after
                // the keyword. If found, braces; otherwise
                // indentation.
                if after.contains('{') {
                    return BlockForm::Braces;
                }
                return BlockForm::Indentation;
            }
        }
    }
    BlockForm::Indentation
}

/// Render a type syntax in canonical form.
fn type_str(ty: &TypeSyntax) -> String {
    match ty {
        TypeSyntax::Named(n) => n.clone(),
        TypeSyntax::Generic { name, args } => {
            let args: Vec<String> = args.iter().map(type_str).collect();
            format!("{}<{}>", name, args.join(", "))
        }
        TypeSyntax::DynTrait { name } => format!("dyn {}", name),
        TypeSyntax::Projection { base, name } => format!("{}::{}", type_str(base), name),
        TypeSyntax::Unknown => "_".to_string(),
    }
}

/// Render an expression as a single line. The formatter does not
/// wrap (ADR 0058 v1), so every expression fits on one line in the
/// output. This is a simplifying restriction: expressions that were
/// multi-line in the source become single-line in the output if
/// they weren't already.
///
/// Recursion into nested block-shaped expressions (If, For, While,
/// Block) is handled by `emit_expr_stmt`, which returns to
/// multi-line emission. `expr_inline` is only called for
/// expression-shaped subexpressions.
fn expr_inline(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(i, _) => format!("{}", i),
        ExprKind::Number(f, _) => format_float(*f),
        ExprKind::String(s, _) => format!("{:?}", s),
        ExprKind::Bool(b, _) => format!("{}", b),
        ExprKind::NullPtr(_) => "null".to_string(),
        ExprKind::PtrLiteral(p, _) => format!("0x{:x}", p),
        ExprKind::Var(name, _) => name.clone(),
        ExprKind::Borrow { expr, .. } => format!("&{}", expr_inline(expr)),
        ExprKind::MutBorrow { expr, .. } => format!("&mut {}", expr_inline(expr)),
        ExprKind::Deref { expr, .. } => format!("*{}", expr_inline(expr)),
        ExprKind::AddrOf { expr, .. } => format!("addr_of({})", expr_inline(expr)),
        ExprKind::Some { value, .. } => format!("Some({})", expr_inline(value)),
        ExprKind::None(_) => "None".to_string(),
        ExprKind::Ok { value, .. } => format!("Ok({})", expr_inline(value)),
        ExprKind::Error { value, .. } => format!("Error({})", expr_inline(value)),
        ExprKind::List(items, _) => {
            let parts: Vec<String> = items.iter().map(expr_inline).collect();
            format!("[{}]", parts.join(", "))
        }
        ExprKind::ArrayAccess { array, index, .. } => {
            format!("{}[{}]", expr_inline(array), expr_inline(index))
        }
        ExprKind::Binary {
            left, op, right, ..
        } => {
            format!(
                "{} {} {}",
                expr_inline(left),
                binop_str(op),
                expr_inline(right)
            )
        }
        ExprKind::Unary { op, expr, .. } => {
            format!("{}{}", unop_str(op), expr_inline(expr))
        }
        ExprKind::FunctionCall { name, args, .. } => {
            let parts: Vec<String> = args.iter().map(expr_inline).collect();
            format!("{}({})", name, parts.join(", "))
        }
        ExprKind::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            let parts: Vec<String> = args.iter().map(expr_inline).collect();
            format!("{}.{}({})", expr_inline(receiver), method, parts.join(", "))
        }
        ExprKind::FieldAccess { object, field, .. } => {
            format!("{}.{}", expr_inline(object), field)
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            let op = if *inclusive { "..=" } else { ".." };
            let s = start.as_ref().map(|e| expr_inline(e)).unwrap_or_default();
            let e = end.as_ref().map(|e| expr_inline(e)).unwrap_or_default();
            format!("{}{}{}", s, op, e)
        }
        ExprKind::RecordLiteral {
            name,
            type_args,
            fields,
            ..
        } => {
            let args = if type_args.is_empty() {
                String::new()
            } else {
                let a: Vec<String> = type_args.iter().map(type_str).collect();
                format!("<{}>", a.join(", "))
            };
            let f: Vec<String> = fields
                .iter()
                .map(|(n, v)| format!("{}: {}", n, expr_inline(v)))
                .collect();
            format!("{}{} {{ {} }}", name, args, f.join(", "))
        }
        ExprKind::MapLiteral {
            key_type,
            value_type,
            entries,
            ..
        } => {
            let prefix = match (key_type, value_type) {
                (Some(k), Some(v)) => format!("Map<{}, {}> ", type_str(k), type_str(v)),
                _ => "Map ".to_string(),
            };
            let e: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", expr_inline(k), expr_inline(v)))
                .collect();
            format!("{}{{ {} }}", prefix, e.join(", "))
        }
        ExprKind::SetLiteral {
            element_type,
            elements,
            ..
        } => {
            let e: Vec<String> = elements.iter().map(expr_inline).collect();
            format!("Set<{}> {{ {} }}", type_str(element_type), e.join(", "))
        }
        // Block-shaped expressions are not inline; they should only
        // appear in statement position. If they reach here, emit
        // them as `{ }` for lack of a better fallback.
        ExprKind::Block { .. }
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::TryCatch { .. }
        | ExprKind::For { .. }
        | ExprKind::While { .. } => {
            // Should not happen for well-formed source. Return a
            // placeholder so the formatter doesn't panic.
            "<block>".to_string()
        }
    }
}

/// ADR 0058 session 4a. Render a `match` pattern as source text.
///
/// A free function (not a method) so it can be called from inside
/// `self.emit_block` closures without an extra immutable borrow of
/// `self`.
fn pattern_text(pat: &crate::frontend::ast::Pattern) -> String {
    use crate::frontend::ast::Pattern;
    match pat {
        Pattern::Some(name) => format!("Some({})", name),
        Pattern::SomeNested(inner) => format!("Some({})", pattern_text(inner)),
        Pattern::None => "None".to_string(),
        Pattern::Ok(name) => format!("Ok({})", name),
        Pattern::OkNested(inner) => format!("Ok({})", pattern_text(inner)),
        Pattern::Error(name) => format!("Error({})", name),
        Pattern::ErrorNested(inner) => format!("Error({})", pattern_text(inner)),
        Pattern::Wildcard => "_".to_string(),
        Pattern::Binding(name) => name.clone(),
        Pattern::Literal(e) => expr_inline(e),
        Pattern::Variant(name) => name.clone(),
        Pattern::Record { name, bindings } => {
            format!("{} {{ {} }}", name, bindings.join(", "))
        }
        Pattern::Guarded { pattern, condition } => {
            format!("{} if {}", pattern_text(pattern), expr_inline(condition))
        }
        Pattern::ListDestructure { first, rest } => {
            let f = first
                .as_ref()
                .map(|p| pattern_text(p))
                .unwrap_or_else(|| "_".to_string());
            let r = rest
                .as_ref()
                .map(|p| pattern_text(p))
                .unwrap_or_else(|| "_".to_string());
            format!("[{}, ...{}]", f, r)
        }
        Pattern::Range { start, end } => {
            let s = start.as_ref().map(|e| expr_inline(e)).unwrap_or_default();
            let e = end.as_ref().map(|e| expr_inline(e)).unwrap_or_default();
            format!("{}..{}", s, e)
        }
    }
}

fn binop_str(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Subtract => "-",
        BinOp::Multiply => "*",
        BinOp::Divide => "/",
        BinOp::Greater => ">",
        BinOp::Less => "<",
        BinOp::GreaterEqual => ">=",
        BinOp::LessEqual => "<=",
        BinOp::Equal => "=",
        BinOp::NotEqual => "!=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::In => "in",
    }
}

fn unop_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "not ",
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    fn parse(source: &str) -> Result<crate::frontend::ast::Program, String> {
        let lexer = Lexer::new(source.to_string()).map_err(|e| format!("{:?}", e))?;
        let mut parser = Parser::new(lexer.tokens);
        parser.parse_program().map_err(|e| format!("{:?}", e))
    }

    fn format(source: &str) -> Result<String, String> {
        let lexer = Lexer::new(source.to_string()).map_err(|e| format!("{:?}", e))?;
        let comments = lexer.comments.clone();
        let mut parser = Parser::new(lexer.tokens);
        let program = parser.parse_program().map_err(|e| format!("{:?}", e))?;
        let f = Formatter::new(&program, source, comments);
        Ok(f.format(&program))
    }

    #[test]
    fn smoke_simple_fn() {
        let source = "fn main() -> Float\n    42.0\n";
        let out = format(source).expect("format");
        eprintln!("--- output ---\n{}--- end ---", out);
        let reparsed = parse(&out).expect("reparse");
        let out2 = {
            let f = Formatter::new(&reparsed, &out, Vec::new());
            f.format(&reparsed)
        };
        assert_eq!(out, out2, "not idempotent");
    }

    #[test]
    fn smoke_simple_proc() {
        let source = "proc main\n    val x := 1\n    print(x)\n";
        let out = format(source).expect("format");
        eprintln!("--- output ---\n{}--- end ---", out);
        let reparsed = parse(&out).expect("reparse");
        let out2 = {
            let f = Formatter::new(&reparsed, &out, Vec::new());
            f.format(&reparsed)
        };
        assert_eq!(out, out2, "not idempotent");
    }

    #[test]
    fn smoke_if_else() {
        let source = "proc main\n    val x := 1\n    if x > 0 then\n        print(x)\n    else\n        print(0)\n";
        let out = format(source).expect("format");
        eprintln!("--- if_else output ---\n{}--- end ---", out);
        let reparsed = parse(&out).expect("reparse");
        let out2 = {
            let f = Formatter::new(&reparsed, &out, Vec::new());
            f.format(&reparsed)
        };
        assert_eq!(out, out2, "not idempotent");
    }

    #[test]
    fn smoke_record() {
        let source = "rec Point\n    x: Int\n    y: Int\n\nproc main\n    var p := Point { x: 1, y: 2 }\n    print(p.x)\n";
        let out = format(source).expect("format");
        eprintln!("--- record output ---\n{}--- end ---", out);
        let reparsed = parse(&out).expect("reparse");
        let out2 = {
            let f = Formatter::new(&reparsed, &out, Vec::new());
            f.format(&reparsed)
        };
        assert_eq!(out, out2, "not idempotent");
    }

    #[test]
    fn smoke_braces_form() {
        let source = "proc main {\n    val x := 1\n    print(x)\n}\n";
        let out = format(source).expect("format");
        eprintln!("--- braces output ---\n{}--- end ---", out);
        let reparsed = parse(&out).expect("reparse");
        let out2 = {
            let f = Formatter::new(&reparsed, &out, Vec::new());
            f.format(&reparsed)
        };
        assert_eq!(out, out2, "not idempotent");
    }

    #[test]
    fn smoke_leading_comment() {
        let source = "// leading\nproc main\n    val x := 1\n    print(x)\n";
        let out = format(source).expect("format");
        eprintln!("--- leading comment output ---\n{}--- end ---", out);
        assert!(out.contains("// leading"), "comment was dropped");
    }

    #[test]
    fn smoke_trailing_comment() {
        let source = "proc main\n    val x := 1 // trailing\n    print(x)\n";
        let out = format(source).expect("format");
        eprintln!("--- trailing comment output ---\n{}--- end ---", out);
        assert!(out.contains("// trailing"), "comment was dropped");
    }
}

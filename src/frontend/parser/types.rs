// src/frontend/parser/types.rs

use super::*;

impl Parser {
    pub(super) fn parse_type_syntax(&mut self) -> Result<TypeSyntax> {
        // ADR 0041. Postfix `::Name` projects through the base type's
        // associated type. Handled here (not in `parse_type_syntax_atom`)
        // so the recursion for `&T` correctly parses `&C::Item` as
        // `Borrow<Projection>`.
        let base = self.parse_type_syntax_atom()?;
        // The lexer emits two adjacent `Colon` tokens for `::`; the
        // `DoubleColon` variant exists but is not currently produced
        // by `Lexer::handle_operator`. Accept either form so a future
        // lexer change is compatible.
        let is_double_colon = matches!(self.peek(), Token::DoubleColon)
            || matches!(
                (self.tokens.get(self.pos), self.tokens.get(self.pos + 1)),
                (Some(a), Some(b))
                    if matches!(a.token, Token::Colon) && matches!(b.token, Token::Colon)
            );
        if is_double_colon {
            // Consume both colons (or the single DoubleColon token).
            if matches!(self.peek(), Token::DoubleColon) {
                self.advance();
            } else {
                self.advance(); // first `:`
                self.advance(); // second `:`
            }
            let assoc_name = self.expect_identifier("associated type name after `::`")?;
            return Ok(TypeSyntax::Projection {
                base: Box::new(base),
                name: assoc_name,
            });
        }
        Ok(base)
    }

    /// The pre-ADR-0041 body of `parse_type_syntax`, renamed to a
    /// private helper. The postfix `::` handling lives in the public
    /// method so it applies uniformly to `Named`, `Generic`, `Self`,
    /// and `dyn Trait` bases.
    fn parse_type_syntax_atom(&mut self) -> Result<TypeSyntax> {
        if matches!(self.peek(), Token::Ampersand) {
            self.advance(); // consume &

            if matches!(self.peek(), Token::Mut) {
                self.advance(); // consume mut
                let inner = self.parse_type_syntax()?;
                return Ok(TypeSyntax::Generic {
                    name: "MutBorrow".to_string(),
                    args: vec![inner],
                });
            }

            let inner = self.parse_type_syntax()?;
            return Ok(TypeSyntax::Generic {
                name: "Borrow".to_string(),
                args: vec![inner],
            });
        }

        if matches!(self.peek(), Token::SelfType) {
            self.advance();
            return Ok(TypeSyntax::Named("Self".to_string()));
        }
        // `dyn` is a soft keyword: it is special only at the head of a
        // type, and remains a valid identifier elsewhere. ADR 0038.
        // `&dyn Trait` reaches here through the `&` branch above,
        // which recurses into this function.
        if let Token::Identifier(name) = self.peek() {
            if name.as_str() == "dyn" {
                self.advance(); // consume `dyn`
                let trait_name = self.expect_identifier("trait name after `dyn`")?;
                return Ok(TypeSyntax::DynTrait { name: trait_name });
            }
        }
        let base = self.expect_identifier("type name")?;
        if matches!(self.peek(), Token::LBracket | Token::Lt) {
            let is_bracket = matches!(self.peek(), Token::LBracket);
            self.advance();
            let mut args = Vec::new();
            while !matches!(
                self.peek(),
                Token::RBracket | Token::Gt | Token::Eof | Token::RParen
            ) {
                args.push(self.parse_type_syntax()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            if is_bracket {
                self.expect_token(Token::RBracket, "']'")?;
            } else {
                self.expect_token(Token::Gt, "'>'")?;
            }
            Ok(TypeSyntax::Generic { name: base, args })
        } else {
            Ok(TypeSyntax::Named(base))
        }
    }

    pub(super) fn parse_type_annotation(&mut self) -> Result<Option<TypeSyntax>> {
        if matches!(self.peek(), Token::Colon) {
            self.advance();
            Ok(Some(self.parse_type_syntax()?))
        } else {
            Ok(None)
        }
    }
}

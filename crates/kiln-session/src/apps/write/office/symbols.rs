// SPDX-License-Identifier: AGPL-3.0-or-later
//! Insert ▸ Symbol from Word's symbol fonts (a `w:sym` that Word shows in the font and Kiln
//! through its Unicode equivalent) and Alt+X, which turns a hexadecimal code before the caret
//! into its character and back.

use kiln_common::{KilnError, Result};
use kiln_doc::{EditKind, Fragment, InlineObject, Op, Pos, Range, Symbol, Transaction};
use kiln_protocol::commands::write_office::SymbolChar;

use super::super::WriteDoc;

/// The symbol fonts the Symbol dialog lists.
pub(super) const SYMBOL_FONTS: [&str; 5] = [
    "Symbol",
    "Wingdings",
    "Wingdings 2",
    "Wingdings 3",
    "Webdings",
];

/// The characters of a symbol font: the codes Word uses (`F020`–`F0FF`) and what they show.
pub(super) fn table(font: &str) -> Vec<SymbolChar> {
    let Some(font) = SYMBOL_FONTS
        .iter()
        .find(|f| f.eq_ignore_ascii_case(font.trim()))
    else {
        return Vec::new();
    };
    (0xF020..=0xF0FF)
        .filter_map(|code| {
            let c = kiln_io_docx::symbol_code_to_unicode(font, code)?;
            (!c.is_control()).then(|| SymbolChar {
                code,
                text: c.to_string(),
            })
        })
        .collect()
}

/// The hexadecimal code at the end of `before` that Alt+X converts: its char count and value.
fn code_before(before: &[char]) -> Option<(usize, u32)> {
    let digits = before
        .iter()
        .rev()
        .take(6)
        .take_while(|c| c.is_ascii_hexdigit())
        .count();
    if digits == 0 {
        return None;
    }
    let start = before.len() - digits;
    let hex: String = before[start..].iter().collect();
    let value = u32::from_str_radix(&hex, 16).ok()?;
    // `U+` in front belongs to the code.
    let prefix =
        if start >= 2 && matches!(before[start - 1], '+') && matches!(before[start - 2], 'U' | 'u')
        {
            2
        } else {
            0
        };
    let ok = char::from_u32(value).is_some_and(|c| !c.is_control() || c == '\t');
    ok.then_some((digits + prefix, value))
}

impl WriteDoc {
    /// Inserts a symbol-font character at the caret, replacing the selection.
    pub(super) fn insert_symbol(&mut self, font: &str, code: u32) -> Result<()> {
        let font = SYMBOL_FONTS
            .iter()
            .find(|f| f.eq_ignore_ascii_case(font.trim()))
            .ok_or_else(|| KilnError::InvalidInput(format!("{font} is not a symbol font")))?;
        if kiln_io_docx::symbol_code_to_unicode(font, code).is_none() {
            return Err(KilnError::InvalidInput(format!(
                "{font} has no character {code:04X}"
            )));
        }
        self.delete_selection()?;
        let at = self.head;
        let props = self.caret_props()?;
        let tx = Transaction::new(vec![Op::InsertInline {
            para: at.para,
            offset: at.offset,
            fragment: Fragment::object(
                InlineObject::Symbol(Symbol {
                    font: (*font).to_owned(),
                    code,
                }),
                props,
            ),
        }])
        .with_kind(EditKind::Typing)
        .with_selection(
            Some(Range::caret(at)),
            Some(Range::caret(Pos::new(at.para, at.offset + 1))),
        );
        self.commit(tx)
    }

    /// Alt+X.
    pub(super) fn toggle_unicode(&mut self) -> Result<()> {
        let (s, e) = self.span();
        let text: Vec<char> = self.para_text(s.para).chars().collect();
        let (start, replacement) = if s != e {
            if s.para != e.para {
                return Ok(());
            }
            let sel = &text[s.offset.min(text.len())..e.offset.min(text.len())];
            match code_before(sel) {
                Some((n, v)) if n == sel.len() => (s.offset, char::from_u32(v).map(String::from)),
                _ if sel.len() == 1 && sel[0] != kiln_doc::OBJECT_CHAR => {
                    (s.offset, Some(format!("U+{:04X}", u32::from(sel[0]))))
                }
                _ => return Ok(()),
            }
        } else {
            let before = &text[..s.offset.min(text.len())];
            match code_before(before) {
                Some((n, v)) => (s.offset - n, char::from_u32(v).map(String::from)),
                None => match before.last() {
                    Some(&c) if c != kiln_doc::OBJECT_CHAR => {
                        (s.offset - 1, Some(format!("{:04X}", u32::from(c))))
                    }
                    _ => return Ok(()),
                },
            }
        };
        let Some(with) = replacement else {
            return Ok(());
        };
        let end = if s == e { s.offset } else { e.offset };
        let para = s.para;
        let props = self.props_at(Pos::new(para, start + 1));
        let after = Pos::new(para, start + with.chars().count());
        self.commit(
            Transaction::new(vec![
                Op::DeleteInline {
                    para,
                    offset: start,
                    len: end - start,
                },
                Op::InsertInline {
                    para,
                    offset: start,
                    fragment: Fragment::text(&with, props),
                },
            ])
            .with_kind(EditKind::Other)
            .with_selection(Some(self.selection_range()), Some(Range::caret(after))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_codes_before_the_caret() {
        let c = |s: &str| code_before(&s.chars().collect::<Vec<_>>());
        assert_eq!(c("snow 2603"), Some((4, 0x2603)));
        assert_eq!(c("U+00E9"), Some((6, 0xE9)));
        assert_eq!(c("x1F600"), Some((5, 0x1_F600)));
        assert_eq!(c("over "), None);
        assert_eq!(c("D800"), None);
        assert!(!table("Wingdings").is_empty());
        assert!(table("Arial").is_empty());
    }
}

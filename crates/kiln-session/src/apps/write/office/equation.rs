// SPDX-License-Identifier: AGPL-3.0-or-later
//! Insert ▸ Equation from the linear form Word's equation editor accepts: `^` superscripts and
//! `_` subscripts (a character, a number, a name or a bracketed group), `/` fractions between two
//! operands, `\sqrt(…)` (or `√`) radicals, `\sum`, `\int` and Greek letters by their names. The
//! equation is written as Office Math (`m:oMath`), which Word edits and Kiln lays out.

use kiln_common::{KilnError, Result};
use kiln_doc::{EditKind, Equation, Fragment, InlineObject, Op, Pos, Range, Transaction};

use super::super::WriteDoc;

/// Names after a backslash and the characters they stand for.
const NAMES: [(&str, char); 40] = [
    ("alpha", 'α'),
    ("beta", 'β'),
    ("gamma", 'γ'),
    ("delta", 'δ'),
    ("epsilon", 'ε'),
    ("zeta", 'ζ'),
    ("eta", 'η'),
    ("theta", 'θ'),
    ("iota", 'ι'),
    ("kappa", 'κ'),
    ("lambda", 'λ'),
    ("mu", 'μ'),
    ("nu", 'ν'),
    ("xi", 'ξ'),
    ("pi", 'π'),
    ("rho", 'ρ'),
    ("sigma", 'σ'),
    ("tau", 'τ'),
    ("phi", 'φ'),
    ("chi", 'χ'),
    ("psi", 'ψ'),
    ("omega", 'ω'),
    ("Gamma", 'Γ'),
    ("Delta", 'Δ'),
    ("Theta", 'Θ'),
    ("Lambda", 'Λ'),
    ("Pi", 'Π'),
    ("Sigma", 'Σ'),
    ("Phi", 'Φ'),
    ("Omega", 'Ω'),
    ("infty", '∞'),
    ("pm", '±'),
    ("times", '×'),
    ("div", '÷'),
    ("le", '≤'),
    ("ge", '≥'),
    ("ne", '≠'),
    ("approx", '≈'),
    ("cdot", '·'),
    ("partial", '∂'),
];

/// N-ary operators.
const NARY: [(&str, char); 4] = [("sum", '∑'), ("prod", '∏'), ("int", '∫'), ("oint", '∮')];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn run(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!("<m:r><m:t>{}</m:t></m:r>", esc(text))
}

struct Parser {
    c: Vec<char>,
    i: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    /// A name after a backslash.
    fn name(&mut self) -> String {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        self.c[start..self.i].iter().collect()
    }

    /// One operand: a bracketed group, a radical, a number, a name, a character.
    fn atom(&mut self) -> Result<String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(KilnError::InvalidInput(
                "the equation nests too deeply".into(),
            ));
        }
        let out = match self.peek() {
            None => String::new(),
            Some('(') => {
                self.i += 1;
                let inner = self.sequence(Some(')'))?;
                if self.peek() == Some(')') {
                    self.i += 1;
                }
                format!("<m:d><m:e>{inner}</m:e></m:d>")
            }
            Some('{') => {
                // Braces group without showing.
                self.i += 1;
                let inner = self.sequence(Some('}'))?;
                if self.peek() == Some('}') {
                    self.i += 1;
                }
                inner
            }
            Some('√') => {
                self.i += 1;
                let arg = self.bare_atom()?;
                format!(
                    "<m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/><m:e>{arg}</m:e></m:rad>"
                )
            }
            Some('\\') => {
                self.i += 1;
                let n = self.name();
                if n == "sqrt" {
                    let arg = self.bare_atom()?;
                    format!(
                        "<m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/><m:e>{arg}</m:e></m:rad>"
                    )
                } else if let Some((_, ch)) = NARY.iter().find(|(k, _)| *k == n) {
                    let (sub, sup) = self.limits()?;
                    let body = self.bare_atom()?;
                    format!(
                        "<m:nary><m:naryPr><m:chr m:val=\"{ch}\"/></m:naryPr><m:sub>{sub}</m:sub><m:sup>{sup}</m:sup><m:e>{body}</m:e></m:nary>"
                    )
                } else if let Some((_, ch)) = NAMES.iter().find(|(k, _)| *k == n) {
                    run(&ch.to_string())
                } else {
                    run(&format!("\\{n}"))
                }
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                    self.i += 1;
                }
                run(&self.c[start..self.i].iter().collect::<String>())
            }
            Some(c) if c.is_alphabetic() => {
                self.i += 1;
                run(&c.to_string())
            }
            Some(c) => {
                self.i += 1;
                run(&c.to_string())
            }
        };
        self.depth -= 1;
        Ok(out)
    }

    /// An operand whose outer brackets do not show (the argument of a script or a fraction).
    fn bare_atom(&mut self) -> Result<String> {
        if self.peek() == Some('(') {
            self.i += 1;
            let inner = self.sequence(Some(')'))?;
            if self.peek() == Some(')') {
                self.i += 1;
            }
            return Ok(inner);
        }
        self.atom()
    }

    /// `_x^y` after an n-ary operator.
    fn limits(&mut self) -> Result<(String, String)> {
        let (mut sub, mut sup) = (String::new(), String::new());
        for _ in 0..2 {
            match self.peek() {
                Some('_') => {
                    self.i += 1;
                    sub = self.bare_atom()?;
                }
                Some('^') => {
                    self.i += 1;
                    sup = self.bare_atom()?;
                }
                _ => break,
            }
        }
        Ok((sub, sup))
    }

    /// An operand with its scripts.
    fn scripted(&mut self) -> Result<String> {
        let base = self.atom()?;
        let mut sub = None;
        let mut sup = None;
        for _ in 0..2 {
            match self.peek() {
                Some('_') if sub.is_none() => {
                    self.i += 1;
                    sub = Some(self.bare_atom()?);
                }
                Some('^') if sup.is_none() => {
                    self.i += 1;
                    sup = Some(self.bare_atom()?);
                }
                _ => break,
            }
        }
        Ok(match (sub, sup) {
            (None, None) => base,
            (Some(b), None) => format!("<m:sSub><m:e>{base}</m:e><m:sub>{b}</m:sub></m:sSub>"),
            (None, Some(p)) => format!("<m:sSup><m:e>{base}</m:e><m:sup>{p}</m:sup></m:sSup>"),
            (Some(b), Some(p)) => {
                format!(
                    "<m:sSubSup><m:e>{base}</m:e><m:sub>{b}</m:sub><m:sup>{p}</m:sup></m:sSubSup>"
                )
            }
        })
    }

    /// Operands up to `end`, with `/` making fractions of the operands around it.
    fn sequence(&mut self, end: Option<char>) -> Result<String> {
        let mut out = String::new();
        let mut last: Option<String> = None;
        while let Some(c) = self.peek() {
            if Some(c) == end {
                break;
            }
            if c == ' ' {
                self.i += 1;
                continue;
            }
            if c == '/'
                && let Some(num) = last.take()
            {
                self.i += 1;
                let den = self.scripted()?;
                let num = strip_delims(&num);
                let den = strip_delims(&den);
                last = Some(format!(
                    "<m:f><m:num>{num}</m:num><m:den>{den}</m:den></m:f>"
                ));
                continue;
            }
            if let Some(prev) = last.take() {
                out.push_str(&prev);
            }
            last = Some(self.scripted()?);
        }
        if let Some(prev) = last {
            out.push_str(&prev);
        }
        Ok(out)
    }
}

/// `(…)` around a fraction's part is the grouping, not part of the equation.
fn strip_delims(s: &str) -> String {
    s.strip_prefix("<m:d><m:e>")
        .and_then(|r| r.strip_suffix("</m:e></m:d>"))
        .map_or_else(|| s.to_owned(), str::to_owned)
}

/// The OMML of a linear equation.
pub(crate) fn linear_to_omml(linear: &str) -> Result<String> {
    let text = linear.trim();
    if text.is_empty() || text.chars().count() > 2000 {
        return Err(KilnError::InvalidInput(
            "an equation has 1 to 2,000 characters".into(),
        ));
    }
    let mut p = Parser {
        c: text.chars().collect(),
        i: 0,
        depth: 0,
    };
    let body = p.sequence(None)?;
    Ok(format!("<m:oMath>{body}</m:oMath>"))
}

impl WriteDoc {
    /// Insert ▸ Equation.
    pub(super) fn insert_equation(&mut self, linear: &str) -> Result<()> {
        let omml = linear_to_omml(linear)?;
        self.delete_selection()?;
        let at = self.head;
        // Alone in its paragraph, an equation is a display equation (centred, Word's default).
        let display = self.para_len(at.para) == 0;
        let omml = if display {
            format!("<m:oMathPara>{omml}</m:oMathPara>")
        } else {
            omml
        };
        let props = self.caret_props()?;
        self.commit(
            Transaction::new(vec![Op::InsertInline {
                para: at.para,
                offset: at.offset,
                fragment: Fragment::object(
                    InlineObject::Equation(Equation { omml, display }),
                    props,
                ),
            }])
            .with_kind(EditKind::Structure)
            .with_selection(
                Some(Range::caret(at)),
                Some(Range::caret(Pos::new(at.para, at.offset + 1))),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_forms_become_office_math() {
        let o = linear_to_omml("a^2+b^2=c^2").unwrap();
        assert_eq!(o.matches("<m:sSup>").count(), 3, "{o}");
        let f = linear_to_omml("(a+b)/2").unwrap();
        assert!(f.contains("<m:f><m:num><m:r><m:t>a</m:t></m:r>"), "{f}");
        assert!(linear_to_omml("x_1").unwrap().contains("<m:sSub>"));
        assert!(linear_to_omml("\\sqrt(x)").unwrap().contains("<m:rad>"));
        assert!(linear_to_omml("\\alpha+\\beta").unwrap().contains("α"));
        let sum = linear_to_omml("\\sum_(i=1)^n i").unwrap();
        assert!(
            sum.contains("<m:nary>") && sum.contains("m:val=\"∑\""),
            "{sum}"
        );
        assert!(linear_to_omml("a<b").unwrap().contains("&lt;"));
        assert!(linear_to_omml("   ").is_err());
        let deep = "(".repeat(200);
        assert!(linear_to_omml(&deep).is_err());
    }
}

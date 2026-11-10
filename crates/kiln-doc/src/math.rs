// SPDX-License-Identifier: AGPL-3.0-or-later
//! Office Math (`m:oMath`) as text: the linear form Kiln Write's lines and Kiln Deck's slides
//! show an equation in (the OMML itself is kept for saving).

/// An equation's text as it shows in a line: Office Math laid out linearly, with superscripts
/// and subscripts as Unicode script characters where they exist (`x²`, `aₙ`), fractions as
/// `a/b`, radicals as `√`, n-ary operators with their limits and brackets with their characters.
#[must_use]
pub fn linear_text(omml: &str) -> String {
    match kiln_ooxml::xml::parse_xml_str(omml) {
        Ok(doc) => {
            let mut out = String::new();
            math(&doc.root, &mut out, 0);
            out
        }
        // Unreadable math: its text runs.
        Err(_) => plain_math_text(omml),
    }
}

fn plain_math_text(omml: &str) -> String {
    let mut out = String::new();
    let mut rest = omml;
    while let Some(i) = rest.find("<m:t") {
        rest = &rest[i + 4..];
        let Some(gt) = rest.find('>') else { break };
        if rest[..gt].ends_with('/') {
            rest = &rest[gt + 1..];
            continue;
        }
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</m:t>") else {
            break;
        };
        out.push_str(&unescape(&rest[..end]));
        rest = &rest[end..];
    }
    out
}

fn sup_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        _ => return None,
    })
}

fn sub_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' | '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'o' => 'ₒ',
        'x' => 'ₓ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'n' => 'ₙ',
        'k' => 'ₖ',
        'm' => 'ₘ',
        't' => 'ₜ',
        _ => return None,
    })
}

/// A script in script characters, or `^(…)`/`_(…)` when one is missing.
fn script(text: &str, sup: bool, out: &mut String) {
    let mapped: Option<String> = text
        .chars()
        .map(|c| if sup { sup_char(c) } else { sub_char(c) })
        .collect();
    match mapped {
        Some(m) if !m.is_empty() => out.push_str(&m),
        _ if text.chars().count() == 1 => {
            out.push(if sup { '^' } else { '_' });
            out.push_str(text);
        }
        _ => {
            out.push_str(if sup { "^(" } else { "_(" });
            out.push_str(text);
            out.push(')');
        }
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn child<'a>(
    e: &'a kiln_ooxml::xml::XmlElement,
    name: &str,
) -> Option<&'a kiln_ooxml::xml::XmlElement> {
    e.elements().find(|c| local(&c.name) == name)
}

/// A property value (`m:chr m:val="∑"` in `m:naryPr`).
fn prop_val<'a>(e: &'a kiln_ooxml::xml::XmlElement, pr: &str, name: &str) -> Option<&'a str> {
    let p = child(e, pr)?;
    let v = child(p, name)?;
    v.attrs.get("m:val").or_else(|| v.attrs.get("val"))
}

fn part(e: &kiln_ooxml::xml::XmlElement, name: &str, depth: usize) -> String {
    let mut s = String::new();
    if let Some(c) = child(e, name) {
        math(c, &mut s, depth + 1);
    }
    s
}

/// Wraps a fraction's part in brackets when it is more than one term.
fn grouped(s: &str) -> String {
    if s.chars().count() <= 1 || s.chars().all(|c| c.is_alphanumeric() || c == '.') {
        s.to_owned()
    } else {
        format!("({s})")
    }
}

fn math(e: &kiln_ooxml::xml::XmlElement, out: &mut String, depth: usize) {
    if depth > 64 {
        return;
    }
    match local(&e.name) {
        "t" => out.push_str(&e.text()),
        "sSup" => {
            out.push_str(&part(e, "e", depth));
            script(&part(e, "sup", depth), true, out);
        }
        "sSub" => {
            out.push_str(&part(e, "e", depth));
            script(&part(e, "sub", depth), false, out);
        }
        "sSubSup" => {
            out.push_str(&part(e, "e", depth));
            script(&part(e, "sub", depth), false, out);
            script(&part(e, "sup", depth), true, out);
        }
        "f" => {
            out.push_str(&grouped(&part(e, "num", depth)));
            out.push('/');
            out.push_str(&grouped(&part(e, "den", depth)));
        }
        "rad" => {
            let deg = part(e, "deg", depth);
            match deg.as_str() {
                "" => out.push('√'),
                "3" => out.push('∛'),
                "4" => out.push('∜'),
                d => {
                    script(d, true, out);
                    out.push('√');
                }
            }
            out.push_str(&grouped(&part(e, "e", depth)));
        }
        "nary" => {
            out.push_str(prop_val(e, "naryPr", "chr").unwrap_or("∫"));
            let sub = part(e, "sub", depth);
            let sup = part(e, "sup", depth);
            if !sub.is_empty() {
                script(&sub, false, out);
            }
            if !sup.is_empty() {
                script(&sup, true, out);
            }
            out.push_str(&part(e, "e", depth));
        }
        "d" => {
            out.push_str(prop_val(e, "dPr", "begChr").unwrap_or("("));
            let sep = prop_val(e, "dPr", "sepChr").unwrap_or("|");
            let mut first = true;
            for c in e.elements().filter(|c| local(&c.name) == "e") {
                if !first {
                    out.push_str(sep);
                }
                first = false;
                math(c, out, depth + 1);
            }
            out.push_str(prop_val(e, "dPr", "endChr").unwrap_or(")"));
        }
        "acc" => {
            out.push_str(&part(e, "e", depth));
            out.push_str(prop_val(e, "accPr", "chr").unwrap_or("\u{302}"));
        }
        "bar" => {
            out.push_str(&part(e, "e", depth));
            out.push('\u{305}');
        }
        // Properties and control data show nothing.
        "rPr" | "ctrlPr" | "sSupPr" | "sSubPr" | "fPr" | "radPr" | "naryPr" | "dPr" | "accPr"
        | "barPr" | "funcPr" | "oMathParaPr" | "argPr" => {}
        "func" => {
            out.push_str(&part(e, "fName", depth));
            out.push('\u{2061}');
            out.push_str(&part(e, "e", depth));
        }
        _ => {
            for c in e.elements() {
                math(c, out, depth + 1);
            }
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// SPDX-License-Identifier: AGPL-3.0-or-later
//! The style index of a text document: common styles (`styles.xml` `office:styles`), automatic
//! styles of `styles.xml` (used by headers and footers) and of `content.xml` (used by the
//! body), default styles, list styles, page layouts and master pages; and named styles mapped
//! to kiln-doc [`Style`]s.
//!
//! Style ids are ODF style names (`Heading_20_1`), display names become the style name
//! (`Heading 1`). A named paragraph style keeps its `style:parent-style-name` as `basedOn` and
//! its `style:next-style-name`; `style:default-outline-level` becomes the outline level and
//! `style:list-style-name` a style-level list reference.

use std::collections::HashMap;

use kiln_doc::numbering::NumberingRef;
use kiln_doc::props::{ParagraphProps, RunProps};
use kiln_doc::st::StyleType;
use kiln_doc::style::{DocDefaults, Style};
use kiln_ooxml::xml::XmlElement;

use crate::props::{FontFaces, read_para_props, read_run_props};
use crate::xml::{kid, kids};

/// Which automatic styles a reference resolves against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `content.xml` (the body).
    Content,
    /// `styles.xml` (master pages: headers and footers).
    Styles,
}

type Key<'a> = (&'a str, &'a str);

/// Every style of a document, by family and name.
#[derive(Debug, Default)]
pub struct StyleIndex<'a> {
    common: HashMap<Key<'a>, &'a XmlElement>,
    common_order: Vec<&'a XmlElement>,
    defaults: HashMap<&'a str, &'a XmlElement>,
    auto_styles: HashMap<Key<'a>, &'a XmlElement>,
    auto_content: HashMap<Key<'a>, &'a XmlElement>,
    /// List styles in definition order (styles.xml common, styles.xml automatic, content.xml
    /// automatic), with their scope.
    pub list_styles: Vec<(&'a str, Scope, &'a XmlElement)>,
    /// `style:page-layout` by name.
    pub page_layouts: HashMap<&'a str, &'a XmlElement>,
    /// `style:master-page` in document order.
    pub master_pages: Vec<&'a XmlElement>,
    /// Data styles (`number:date-style`, …) by name.
    pub data_styles: HashMap<&'a str, &'a XmlElement>,
    /// Font face declarations.
    pub fonts: FontFaces,
    /// `text:outline-style`, if any.
    pub outline_style: Option<&'a XmlElement>,
}

fn index_styles<'a>(
    container: &'a XmlElement,
    map: &mut HashMap<Key<'a>, &'a XmlElement>,
    order: Option<&mut Vec<&'a XmlElement>>,
    lists: &mut Vec<(&'a str, Scope, &'a XmlElement)>,
    scope: Scope,
    data: &mut HashMap<&'a str, &'a XmlElement>,
    layouts: &mut HashMap<&'a str, &'a XmlElement>,
) {
    let mut order = order;
    for e in container.elements() {
        let Some(name) = e.attr("style:name") else {
            continue;
        };
        match e.name.as_str() {
            "style:style" => {
                let family = e.attr("style:family").unwrap_or_default();
                map.insert((family, name), e);
                if let Some(o) = order.as_deref_mut() {
                    o.push(e);
                }
            }
            "text:list-style" => lists.push((name, scope, e)),
            "style:page-layout" => {
                layouts.insert(name, e);
            }
            n if n.starts_with("number:") => {
                data.insert(name, e);
            }
            _ => {}
        }
    }
}

impl<'a> StyleIndex<'a> {
    /// Indexes the styles of `styles.xml` and `content.xml` (either may be missing).
    #[must_use]
    pub fn new(styles: Option<&'a XmlElement>, content: Option<&'a XmlElement>) -> Self {
        let mut ix = StyleIndex::default();
        for root in [styles, content].into_iter().flatten() {
            ix.fonts.add_decls(root);
        }
        if let Some(s) = styles {
            if let Some(common) = kid(s, "office:styles") {
                for d in kids(common, "style:default-style") {
                    if let Some(f) = d.attr("style:family") {
                        ix.defaults.insert(f, d);
                    }
                }
                index_styles(
                    common,
                    &mut ix.common,
                    Some(&mut ix.common_order),
                    &mut ix.list_styles,
                    Scope::Styles,
                    &mut ix.data_styles,
                    &mut ix.page_layouts,
                );
                ix.outline_style = kid(common, "text:outline-style");
            }
            if let Some(auto) = kid(s, "office:automatic-styles") {
                index_styles(
                    auto,
                    &mut ix.auto_styles,
                    None,
                    &mut ix.list_styles,
                    Scope::Styles,
                    &mut ix.data_styles,
                    &mut ix.page_layouts,
                );
            }
            if let Some(m) = kid(s, "office:master-styles") {
                ix.master_pages = kids(m, "style:master-page").collect();
            }
        }
        if let Some(c) = content
            && let Some(auto) = kid(c, "office:automatic-styles")
        {
            index_styles(
                auto,
                &mut ix.auto_content,
                None,
                &mut ix.list_styles,
                Scope::Content,
                &mut ix.data_styles,
                &mut ix.page_layouts,
            );
        }
        ix
    }

    /// A style by family and name: the scope's automatic styles first, then common styles.
    #[must_use]
    pub fn get(&self, scope: Scope, family: &str, name: &str) -> Option<&'a XmlElement> {
        let autos = match scope {
            Scope::Content => &self.auto_content,
            Scope::Styles => &self.auto_styles,
        };
        autos
            .get(&(family, name))
            .or_else(|| self.common.get(&(family, name)))
            .copied()
    }

    /// An automatic style of the scope (not a common style of the same name).
    #[must_use]
    pub fn get_auto(&self, scope: Scope, family: &str, name: &str) -> Option<&'a XmlElement> {
        match scope {
            Scope::Content => &self.auto_content,
            Scope::Styles => &self.auto_styles,
        }
        .get(&(family, name))
        .copied()
    }

    /// Whether a style is a common (named) style.
    #[must_use]
    pub fn is_common(&self, family: &str, name: &str) -> bool {
        self.common.contains_key(&(family, name))
    }

    /// The default style of a family.
    #[must_use]
    pub fn default_style(&self, family: &str) -> Option<&'a XmlElement> {
        self.defaults.get(family).copied()
    }

    /// A list style by name, preferring the scope's automatic list styles.
    #[must_use]
    pub fn list_style(&self, scope: Scope, name: &str) -> Option<usize> {
        self.list_styles
            .iter()
            .position(|(n, s, _)| *n == name && *s == scope)
            .or_else(|| self.list_styles.iter().position(|(n, _, _)| *n == name))
    }

    /// A master page by name.
    #[must_use]
    pub fn master_page(&self, name: &str) -> Option<&'a XmlElement> {
        self.master_pages
            .iter()
            .find(|m| m.attr("style:name") == Some(name))
            .copied()
    }

    /// The attribute of a properties element along a style's parent chain (and the family's
    /// default style).
    #[must_use]
    pub fn chain_attr(
        &self,
        scope: Scope,
        family: &str,
        name: &str,
        props: &str,
        attr: &str,
    ) -> Option<&'a str> {
        let mut cur = self.get(scope, family, name);
        let mut guard = 0;
        while let Some(s) = cur {
            if let Some(v) = kid(s, props).and_then(|p| p.attr(attr)) {
                return Some(v);
            }
            guard += 1;
            if guard > 64 {
                return None;
            }
            cur = s
                .attr("style:parent-style-name")
                .and_then(|p| self.get(scope, family, p));
        }
        self.default_style(family)
            .and_then(|d| kid(d, props))
            .and_then(|p| p.attr(attr))
    }

    /// The paragraph style's outline level along the chain (`style:default-outline-level`).
    #[must_use]
    pub fn outline_level(&self, scope: Scope, name: &str) -> Option<u8> {
        let mut cur = self.get(scope, "paragraph", name);
        let mut guard = 0;
        while let Some(s) = cur {
            if let Some(v) = s.attr("style:default-outline-level") {
                return v.parse::<u8>().ok().filter(|l| (1..=10).contains(l));
            }
            guard += 1;
            if guard > 64 {
                return None;
            }
            cur = s
                .attr("style:parent-style-name")
                .and_then(|p| self.get(scope, "paragraph", p));
        }
        None
    }

    /// The document defaults from the paragraph family's default style.
    #[must_use]
    pub fn doc_defaults(&self) -> DocDefaults {
        let mut d = DocDefaults::default();
        if let Some(def) = self.default_style("paragraph") {
            if let Some(pp) = kid(def, "style:paragraph-properties") {
                d.ppr = read_para_props(pp);
                // Tab stops of the default style are the default tab interval, not stops.
                d.ppr.tabs = None;
            }
            if let Some(tp) = kid(def, "style:text-properties") {
                d.rpr = read_run_props(tp, &self.fonts);
            }
        }
        d
    }

    /// The default tab interval in twips (`style:tab-stop-distance` of the default style).
    #[must_use]
    pub fn default_tab_stop(&self) -> Option<i32> {
        self.default_style("paragraph")
            .and_then(|d| kid(d, "style:paragraph-properties"))
            .and_then(|p| p.attr("style:tab-stop-distance"))
            .and_then(crate::units::twips)
            .filter(|t| *t > 0)
    }

    /// Named paragraph and text styles as model styles, in file order. `list_id` maps a list
    /// style name to its list instance id.
    pub fn named_styles(&self, list_id: impl Fn(&str) -> Option<u32>) -> Vec<Style> {
        let mut out = Vec::new();
        for s in &self.common_order {
            let (Some(name), Some(family)) = (s.attr("style:name"), s.attr("style:family"))
            else {
                continue;
            };
            let kind = match family {
                "paragraph" => StyleType::Paragraph,
                "text" => StyleType::Character,
                _ => continue,
            };
            let mut st = Style::new(name, kind, s.attr("style:display-name").unwrap_or(name));
            st.based_on = s
                .attr("style:parent-style-name")
                .filter(|p| self.is_common(family, p))
                .map(str::to_owned);
            st.next = s
                .attr("style:next-style-name")
                .filter(|n| *n != name && self.is_common(family, n))
                .map(str::to_owned);
            st.custom = s.attr("style:class").is_none_or(|c| c != "text" && c != "extra");
            st.hidden = s.attr("style:hidden") == Some("true");
            if let Some(tp) = kid(s, "style:text-properties") {
                let r = read_run_props(tp, &self.fonts);
                if r != RunProps::default() {
                    st.rpr = Some(r);
                }
            }
            if kind == StyleType::Paragraph {
                let mut p = kid(s, "style:paragraph-properties")
                    .map(read_para_props)
                    .unwrap_or_default();
                if let Some(l) = s
                    .attr("style:default-outline-level")
                    .and_then(|v| v.parse::<u8>().ok())
                    .filter(|l| (1..=9).contains(l))
                {
                    p.outline_level = Some(l - 1);
                }
                if let Some(id) = s.attr("style:list-style-name").and_then(&list_id) {
                    p.numbering = Some(NumberingRef {
                        num_id: Some(id),
                        level: Some(p.outline_level.unwrap_or(0)),
                    });
                }
                if p != ParagraphProps::default() {
                    st.ppr = Some(p);
                }
                st.is_default = name == "Standard";
            }
            out.push(st);
        }
        out
    }
}

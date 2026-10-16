// SPDX-License-Identifier: AGPL-3.0-or-later
//! List numbering (`numbering.xml`): abstract definitions (`w:abstractNum`) with nine levels,
//! instances (`w:num`) with level overrides, and the label computation Word performs.
//!
//! Counting model (matching Word; MS-OI29500 §2.1.299 and §2.1.302):
//!
//! - counters belong to the **abstract** definition, so two instances of one abstract definition
//!   continue one sequence ("Continue numbering");
//! - an instance with `w:startOverride` restarts the levels it overrides the first time it is used
//!   ("Restart at 1" creates such an instance);
//! - incrementing level *n* resets deeper levels unless a deeper level's `w:lvlRestart` says
//!   otherwise (0 = never restart, *k* = restart only after level *k* (1-based) or a shallower
//!   one);
//! - `%k` placeholders of levels not used yet in the sequence render that level's start value;
//! - `w:isLgl` renders every referenced level in decimal (legal numbering).

pub mod formats;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::DocError;
use crate::props::{Indent, ParagraphProps, RunFonts, RunProps};
use crate::st::{LevelAlignment, LevelSuffix, MultiLevelType, NumberFormat};
use crate::style::StyleSheet;
use formats::{bullet_display_text, format_custom, format_number};

/// The deepest list level (levels are 0–8).
pub const MAX_LEVEL: u8 = 8;

/// Clamps a list level into 0–8.
#[must_use]
pub fn clamp_level(level: i64) -> u8 {
    u8::try_from(level.clamp(0, i64::from(MAX_LEVEL))).unwrap_or(0)
}

/// One level of a list definition (`w:lvl`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Level {
    /// Level index 0–8 (`w:ilvl`).
    pub level: u8,
    /// `w:start`.
    pub start: i64,
    /// `w:numFmt`.
    pub format: NumberFormat,
    /// `w:numFmt/@w:format` for `custom` formats (`001, 002, 003, …`).
    pub custom_format: Option<String>,
    /// `w:lvlText`: a template such as `%1.%2.`, or the literal bullet character.
    pub text: String,
    /// `w:lvlRestart`: `None` restarts after any shallower level; 0 never; *k* after level *k*.
    pub restart_after: Option<u8>,
    /// `w:isLgl`: legal numbering.
    pub is_legal: bool,
    /// `w:suff`.
    pub suffix: LevelSuffix,
    /// `w:lvlJc`.
    pub alignment: LevelAlignment,
    /// `w:pStyle`: the paragraph style linked to this level.
    pub p_style: Option<String>,
    /// `w:lvlPicBulletId`, preserved.
    pub pic_bullet_id: Option<u32>,
    /// `w:pPr` (indents, tabs).
    pub ppr: Option<ParagraphProps>,
    /// `w:rPr`: formatting of the label itself.
    pub rpr: Option<RunProps>,
}

impl Level {
    /// A level with a start of 1, a format and a template.
    #[must_use]
    pub fn new(level: u8, format: NumberFormat, text: &str) -> Self {
        Self {
            level,
            start: 1,
            format,
            text: text.to_owned(),
            ..Self::default()
        }
    }
}

/// An abstract list definition (`w:abstractNum`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AbstractNum {
    /// `w:abstractNumId`.
    pub id: u32,
    /// `w:nsid` (hex): how Word recognises the same list across documents.
    pub nsid: Option<String>,
    /// `w:multiLevelType`.
    pub multi_level_type: Option<MultiLevelType>,
    /// `w:name`.
    pub name: Option<String>,
    /// `w:styleLink`: this definition is the list style named here.
    pub style_link: Option<String>,
    /// `w:numStyleLink`: this definition defers to the list style named here.
    pub num_style_link: Option<String>,
    /// Levels sorted by index.
    pub levels: Vec<Level>,
}

/// A level override of an instance (`w:lvlOverride`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LevelOverride {
    /// The level.
    pub level: u8,
    /// `w:startOverride`.
    pub start_override: Option<i64>,
    /// A full replacement level (`w:lvlOverride/w:lvl`).
    pub definition: Option<Level>,
}

/// A list instance (`w:num`), what paragraphs reference by `w:numId`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NumInstance {
    /// `w:numId` (1 or more; 0 means "no list").
    pub id: u32,
    /// `w:abstractNumId`.
    pub abstract_id: u32,
    /// `w:lvlOverride`s.
    pub overrides: Vec<LevelOverride>,
}

/// The list definitions of a document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Numbering {
    /// Abstract definitions by id.
    pub abstracts: BTreeMap<u32, AbstractNum>,
    /// Instances by `numId`.
    pub instances: BTreeMap<u32, NumInstance>,
}

impl Numbering {
    /// No lists.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an abstract definition (levels are sorted); `id` `None` takes the next free id.
    pub fn add_abstract(&mut self, mut def: AbstractNum, id: Option<u32>) -> &AbstractNum {
        let id = id.unwrap_or_else(|| next_free(&self.abstracts, 0));
        def.id = id;
        def.levels.sort_by_key(|l| l.level);
        self.abstracts.insert(id, def);
        &self.abstracts[&id]
    }

    /// Adds an instance of an abstract definition; `id` `None` takes the next free id (from 1).
    ///
    /// # Errors
    ///
    /// [`DocError::UnknownList`] when the abstract definition does not exist.
    pub fn add_instance(
        &mut self,
        abstract_id: u32,
        overrides: Vec<LevelOverride>,
        id: Option<u32>,
    ) -> Result<&NumInstance, DocError> {
        if !self.abstracts.contains_key(&abstract_id) {
            return Err(DocError::UnknownList(format!("abstractNum {abstract_id}")));
        }
        let id = id.unwrap_or_else(|| next_free(&self.instances, 1));
        self.instances.insert(
            id,
            NumInstance {
                id,
                abstract_id,
                overrides,
            },
        );
        Ok(&self.instances[&id])
    }

    /// The abstract definition `num_id` uses, following `w:numStyleLink` through list styles
    /// (`styles` resolves a list style to its `numId`; cycles stop).
    #[must_use]
    pub fn abstract_for(&self, num_id: u32, styles: Option<&StyleSheet>) -> Option<&AbstractNum> {
        let inst = self.instances.get(&num_id)?;
        let mut abs = self.abstracts.get(&inst.abstract_id)?;
        let mut seen = HashSet::new();
        while let (Some(link), Some(styles)) = (abs.num_style_link.as_deref(), styles) {
            if !seen.insert(abs.id) {
                break;
            }
            let linked_num = styles
                .get(link)
                .and_then(|s| s.ppr.as_ref())
                .and_then(|p| p.numbering)
                .and_then(|n| n.num_id);
            let next = linked_num
                .and_then(|n| self.instances.get(&n))
                .and_then(|i| self.abstracts.get(&i.abstract_id));
            match next {
                Some(n) => abs = n,
                None => break,
            }
        }
        Some(abs)
    }

    /// The effective definition of `level` (0–8, clamped) of `num_id`, after `w:lvlOverride/w:lvl`.
    #[must_use]
    pub fn level(&self, num_id: u32, level: u8, styles: Option<&StyleSheet>) -> Option<&Level> {
        let abs = self.abstract_for(num_id, styles)?;
        let lvl = level.min(MAX_LEVEL);
        let over = self
            .instances
            .get(&num_id)
            .and_then(|i| i.overrides.iter().find(|o| o.level == lvl))
            .and_then(|o| o.definition.as_ref());
        over.or_else(|| abs.levels.iter().find(|l| l.level == lvl))
    }

    /// The nine effective levels of `num_id` (missing levels are `None`).
    #[must_use]
    pub fn levels(&self, num_id: u32, styles: Option<&StyleSheet>) -> [Option<&Level>; 9] {
        std::array::from_fn(|i| self.level(num_id, clamp_level(i as i64), styles))
    }

    /// The level whose `w:pStyle` is `style_id` (Word's style-linked outline numbering).
    #[must_use]
    pub fn level_for_style(
        &self,
        num_id: u32,
        style_id: &str,
        styles: Option<&StyleSheet>,
    ) -> Option<u8> {
        self.levels(num_id, styles)
            .into_iter()
            .flatten()
            .find(|l| l.p_style.as_deref() == Some(style_id))
            .map(|l| l.level)
    }

    /// A new instance that restarts `num_id`'s list ("Restart at 1"), at `start_at` or the level's
    /// start.
    ///
    /// # Errors
    ///
    /// [`DocError::UnknownList`] for an unknown `num_id`.
    pub fn restart(
        &mut self,
        num_id: u32,
        level: u8,
        start_at: Option<i64>,
    ) -> Result<&NumInstance, DocError> {
        let inst = self
            .instances
            .get(&num_id)
            .ok_or_else(|| DocError::UnknownList(format!("numId {num_id}")))?;
        let abstract_id = inst.abstract_id;
        let start = start_at
            .or_else(|| self.level(num_id, level, None).map(|l| l.start))
            .unwrap_or(1);
        self.add_instance(
            abstract_id,
            vec![LevelOverride {
                level: level.min(MAX_LEVEL),
                start_override: Some(start),
                definition: None,
            }],
            None,
        )
    }

    /// Creates a bulleted list with Word's default bullet library (•, o, ▪ repeating).
    pub fn create_bullet_list(&mut self) -> &NumInstance {
        let glyphs = [
            ("\u{F0B7}", "Symbol"),
            ("o", "Courier New"),
            ("\u{F0A7}", "Wingdings"),
        ];
        let levels = (0..=MAX_LEVEL)
            .map(|i| {
                let (text, font) = glyphs[usize::from(i % 3)];
                let mut fonts = RunFonts::latin(font);
                if font == "Courier New" {
                    fonts.cs = Some(font.to_owned());
                }
                Level {
                    ppr: Some(indent_ppr(720 * (i32::from(i) + 1), 360)),
                    rpr: Some(RunProps {
                        fonts: Some(fonts),
                        ..RunProps::default()
                    }),
                    ..Level::new(i, NumberFormat::Bullet, text)
                }
            })
            .collect();
        self.add_list(MultiLevelType::HybridMultilevel, levels)
    }

    /// Creates a numbered list like Word's default numbering library (1. a. i. repeating).
    pub fn create_numbered_list(&mut self) -> &NumInstance {
        let fmts = [
            (NumberFormat::Decimal, LevelAlignment::Left),
            (NumberFormat::LowerLetter, LevelAlignment::Left),
            (NumberFormat::LowerRoman, LevelAlignment::Right),
        ];
        let levels = (0..=MAX_LEVEL)
            .map(|i| {
                let (format, alignment) = fmts[usize::from(i % 3)].clone();
                let hanging = if alignment == LevelAlignment::Right {
                    180
                } else {
                    360
                };
                Level {
                    alignment,
                    ppr: Some(indent_ppr(720 * (i32::from(i) + 1), hanging)),
                    ..Level::new(i, format, &format!("%{}.", i + 1))
                }
            })
            .collect();
        self.add_list(MultiLevelType::HybridMultilevel, levels)
    }

    /// Creates Word's "1. / 1.1. / 1.1.1." multilevel list, optionally with legal numbering.
    pub fn create_multilevel_list(&mut self, legal: bool) -> &NumInstance {
        let levels = (0..=MAX_LEVEL)
            .map(|i| {
                let text: Vec<String> = (1..=i + 1).map(|k| format!("%{k}")).collect();
                let li = i32::from(i);
                Level {
                    is_legal: legal,
                    ppr: Some(indent_ppr(360 + li * 432, 360 + li * 72)),
                    ..Level::new(i, NumberFormat::Decimal, &format!("{}.", text.join(".")))
                }
            })
            .collect();
        self.add_list(MultiLevelType::Multilevel, levels)
    }

    fn add_list(&mut self, kind: MultiLevelType, levels: Vec<Level>) -> &NumInstance {
        let nsid = self.nsid();
        let abs_id = next_free(&self.abstracts, 0);
        self.add_abstract(
            AbstractNum {
                nsid: Some(nsid),
                multi_level_type: Some(kind),
                levels,
                ..AbstractNum::default()
            },
            Some(abs_id),
        );
        let id = next_free(&self.instances, 1);
        self.instances.insert(
            id,
            NumInstance {
                id,
                abstract_id: abs_id,
                overrides: Vec::new(),
            },
        );
        &self.instances[&id]
    }

    /// A deterministic `w:nsid` per definition count, so documents diff stably.
    fn nsid(&self) -> String {
        let n = u32::try_from(self.abstracts.len()).unwrap_or(u32::MAX);
        format!(
            "{:08X}",
            0x1A2B_3C4Du32.wrapping_add(n.wrapping_mul(0x9E37_79B1))
        )
    }
}

fn indent_ppr(left: i32, hanging: i32) -> ParagraphProps {
    ParagraphProps {
        indent: Some(Indent {
            left: Some(left),
            hanging: Some(hanging),
            ..Indent::default()
        }),
        ..ParagraphProps::default()
    }
}

fn next_free<V>(map: &BTreeMap<u32, V>, min: u32) -> u32 {
    map.keys()
        .next_back()
        .map_or(min, |k| k.saturating_add(1).max(min))
}

/// Renders a level's label for the given counters (index = level, value = current count). Levels
/// without a counter use their start value.
#[must_use]
pub fn format_level_text(levels: &[Option<&Level>], level: u8, counters: &[Option<i64>]) -> String {
    let Some(Some(lvl)) = levels.get(usize::from(level)) else {
        return String::new();
    };
    let has_placeholder = has_placeholder(&lvl.text);
    if lvl.format == NumberFormat::Bullet || (lvl.format == NumberFormat::None && !has_placeholder)
    {
        return lvl.text.clone();
    }
    let mut out = String::with_capacity(lvl.text.len() + 8);
    let mut chars = lvl.text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%'
            && let Some(d) = chars
                .peek()
                .and_then(|d| d.to_digit(10))
                .filter(|d| *d >= 1)
        {
            chars.next();
            let k = (d - 1) as usize;
            let Some(Some(r)) = levels.get(k) else {
                continue;
            };
            if r.format == NumberFormat::None {
                continue;
            }
            let value = counters.get(k).copied().flatten().unwrap_or(r.start);
            let mut fmt = r.format.clone();
            if lvl.is_legal && fmt != NumberFormat::DecimalZero {
                fmt = NumberFormat::Decimal;
            }
            if fmt == NumberFormat::Bullet {
                fmt = NumberFormat::Decimal;
            }
            if fmt == NumberFormat::Custom
                && let Some(p) = &r.custom_format
            {
                out.push_str(&format_custom(value, p));
            } else {
                out.push_str(&format_number(value, &fmt));
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn has_placeholder(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && chars.peek().is_some_and(|d| matches!(d, '1'..='9')) {
            return true;
        }
    }
    false
}

/// A computed list label for one paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLabel {
    /// The label as displayed (bullets mapped to Unicode).
    pub text: String,
    /// The label text before bullet glyph mapping.
    pub raw_text: String,
    /// The instance.
    pub num_id: u32,
    /// The abstract definition whose counters were used.
    pub abstract_id: u32,
    /// The level.
    pub level: u8,
    /// Counter values of levels 0..=level at this paragraph.
    pub counters: Vec<i64>,
    /// True for bullets.
    pub is_bullet: bool,
    /// What follows the label.
    pub suffix: LevelSuffix,
    /// Label alignment within the indent.
    pub alignment: LevelAlignment,
    /// The label's run properties from the level definition.
    pub rpr: Option<RunProps>,
}

/// Counts list paragraphs in document order. [`ListCounter::next`] returns the label of the next
/// paragraph in a list.
#[derive(Debug)]
pub struct ListCounter<'a> {
    numbering: &'a Numbering,
    styles: Option<&'a StyleSheet>,
    counters: BTreeMap<u32, [Option<i64>; 9]>,
    seen_instances: HashSet<u32>,
}

impl<'a> ListCounter<'a> {
    /// A counter over a document's lists; `styles` resolves `w:numStyleLink`.
    #[must_use]
    pub fn new(numbering: &'a Numbering, styles: Option<&'a StyleSheet>) -> Self {
        Self {
            numbering,
            styles,
            counters: BTreeMap::new(),
            seen_instances: HashSet::new(),
        }
    }

    /// The label of the next paragraph in list `num_id` at `level` (clamped to 0–8), or `None`
    /// for `num_id` 0, an unknown list or an undefined level.
    pub fn next(&mut self, num_id: u32, level: u8) -> Option<ListLabel> {
        if num_id == 0 {
            return None;
        }
        let abs = self.numbering.abstract_for(num_id, self.styles)?;
        let inst = self.numbering.instances.get(&num_id)?;
        let lvl = level.min(MAX_LEVEL);
        let levels = self.numbering.levels(num_id, self.styles);
        let def = levels[usize::from(lvl)]?;
        let counters = self.counters.entry(abs.id).or_insert([None; 9]);
        if self.seen_instances.insert(num_id) {
            for o in &inst.overrides {
                if o.start_override.is_some() || o.definition.is_some() {
                    counters[usize::from(o.level.min(MAX_LEVEL))] = None;
                }
            }
        }
        let start_of = |i: usize| {
            inst.overrides
                .iter()
                .find(|o| usize::from(o.level) == i)
                .and_then(|o| o.start_override)
                .or_else(|| levels[i].map(|l| l.start))
                .unwrap_or(1)
        };
        let li = usize::from(lvl);
        counters[li] = Some(match counters[li] {
            None => start_of(li),
            Some(c) => c.saturating_add(1),
        });
        for (d, slot) in counters.iter_mut().enumerate().skip(li + 1) {
            let restart = match levels[d].and_then(|l| l.restart_after) {
                None => true,
                Some(0) => false,
                Some(k) => usize::from(lvl) < usize::from(k),
            };
            if restart {
                *slot = None;
            }
        }
        let effective: [Option<i64>; 9] = std::array::from_fn(|i| match counters[i] {
            None if levels[i].is_some() => Some(start_of(i)),
            other => other,
        });
        let raw_text = format_level_text(&levels, lvl, &effective);
        let is_bullet = def.format == NumberFormat::Bullet;
        let font = def
            .rpr
            .as_ref()
            .and_then(|r| r.fonts.as_ref())
            .and_then(|f| f.ascii.as_deref().or(f.h_ansi.as_deref()));
        Some(ListLabel {
            text: if is_bullet {
                bullet_display_text(&raw_text, font)
            } else {
                raw_text.clone()
            },
            raw_text,
            num_id,
            abstract_id: abs.id,
            level: lvl,
            counters: effective[..=li].iter().map(|c| c.unwrap_or(0)).collect(),
            is_bullet,
            suffix: def.suffix,
            alignment: def.alignment,
            rpr: def.rpr.clone(),
        })
    }
}

#[cfg(test)]
mod tests;

// SPDX-License-Identifier: AGPL-3.0-or-later
//! PowerPoint's display names of the preset shapes ("Rectangle: Rounded Corners", "Flowchart:
//! Process"), for the Shapes gallery, the Selection Pane and the names of new shapes.

/// `(preset, display name)`, sorted by preset.
const NAMES: &[(&str, &str)] = &[
    (
        "accentBorderCallout1",
        "Callout: Line with Border and Accent Bar",
    ),
    (
        "accentBorderCallout2",
        "Callout: Bent Line with Border and Accent Bar",
    ),
    (
        "accentBorderCallout3",
        "Callout: Double Bent Line with Border and Accent Bar",
    ),
    ("accentCallout1", "Callout: Line with Accent Bar"),
    ("accentCallout2", "Callout: Bent Line with Accent Bar"),
    (
        "accentCallout3",
        "Callout: Double Bent Line with Accent Bar",
    ),
    (
        "actionButtonBackPrevious",
        "Action Button: Go Back or Previous",
    ),
    ("actionButtonBeginning", "Action Button: Go to Beginning"),
    ("actionButtonBlank", "Action Button: Blank"),
    ("actionButtonDocument", "Action Button: Document"),
    ("actionButtonEnd", "Action Button: Go to End"),
    (
        "actionButtonForwardNext",
        "Action Button: Go Forward or Next",
    ),
    ("actionButtonHelp", "Action Button: Help"),
    ("actionButtonHome", "Action Button: Go Home"),
    ("actionButtonInformation", "Action Button: Get Information"),
    ("actionButtonMovie", "Action Button: Video"),
    ("actionButtonReturn", "Action Button: Return"),
    ("actionButtonSound", "Action Button: Sound"),
    ("arc", "Arc"),
    ("bentArrow", "Arrow: Bent"),
    ("bentConnector2", "Connector: Elbow"),
    ("bentConnector3", "Connector: Elbow"),
    ("bentConnector4", "Connector: Elbow"),
    ("bentConnector5", "Connector: Elbow"),
    ("bentUpArrow", "Arrow: Bent-Up"),
    ("bevel", "Rectangle: Beveled"),
    ("blockArc", "Block Arc"),
    ("borderCallout1", "Callout: Line"),
    ("borderCallout2", "Callout: Bent Line"),
    ("borderCallout3", "Callout: Double Bent Line"),
    ("bracePair", "Double Brace"),
    ("bracketPair", "Double Bracket"),
    ("callout1", "Callout: Line with No Border"),
    ("callout2", "Callout: Bent Line with No Border"),
    ("callout3", "Callout: Double Bent Line with No Border"),
    ("can", "Cylinder"),
    ("chartPlus", "Chart Plus"),
    ("chartStar", "Chart Star"),
    ("chartX", "Chart X"),
    ("chevron", "Arrow: Chevron"),
    ("chord", "Chord"),
    ("circularArrow", "Arrow: Circular"),
    ("cloud", "Cloud"),
    ("cloudCallout", "Thought Bubble: Cloud"),
    ("corner", "L-Shape"),
    ("cornerTabs", "Corner Tabs"),
    ("cube", "Cube"),
    ("curvedConnector2", "Connector: Curved"),
    ("curvedConnector3", "Connector: Curved"),
    ("curvedConnector4", "Connector: Curved"),
    ("curvedConnector5", "Connector: Curved"),
    ("curvedDownArrow", "Arrow: Curved Down"),
    ("curvedLeftArrow", "Arrow: Curved Left"),
    ("curvedRightArrow", "Arrow: Curved Right"),
    ("curvedUpArrow", "Arrow: Curved Up"),
    ("decagon", "Decagon"),
    ("diagStripe", "Diagonal Stripe"),
    ("diamond", "Diamond"),
    ("dodecagon", "Dodecagon"),
    ("donut", "Circle: Hollow"),
    ("doubleWave", "Double Wave"),
    ("downArrow", "Arrow: Down"),
    ("downArrowCallout", "Callout: Down Arrow"),
    ("ellipse", "Oval"),
    ("ellipseRibbon", "Ribbon: Curved and Tilted Down"),
    ("ellipseRibbon2", "Ribbon: Curved and Tilted Up"),
    ("flowChartAlternateProcess", "Flowchart: Alternate Process"),
    ("flowChartCollate", "Flowchart: Collate"),
    ("flowChartConnector", "Flowchart: Connector"),
    ("flowChartDecision", "Flowchart: Decision"),
    ("flowChartDelay", "Flowchart: Delay"),
    ("flowChartDisplay", "Flowchart: Display"),
    ("flowChartDocument", "Flowchart: Document"),
    ("flowChartExtract", "Flowchart: Extract"),
    ("flowChartInputOutput", "Flowchart: Data"),
    ("flowChartInternalStorage", "Flowchart: Internal Storage"),
    ("flowChartMagneticDisk", "Flowchart: Magnetic Disk"),
    ("flowChartMagneticDrum", "Flowchart: Direct Access Storage"),
    (
        "flowChartMagneticTape",
        "Flowchart: Sequential Access Storage",
    ),
    ("flowChartManualInput", "Flowchart: Manual Input"),
    ("flowChartManualOperation", "Flowchart: Manual Operation"),
    ("flowChartMerge", "Flowchart: Merge"),
    ("flowChartMultidocument", "Flowchart: Multidocument"),
    ("flowChartOfflineStorage", "Flowchart: Stored Data"),
    ("flowChartOffpageConnector", "Flowchart: Off-page Connector"),
    ("flowChartOnlineStorage", "Flowchart: Stored Data"),
    ("flowChartOr", "Flowchart: Or"),
    (
        "flowChartPredefinedProcess",
        "Flowchart: Predefined Process",
    ),
    ("flowChartPreparation", "Flowchart: Preparation"),
    ("flowChartProcess", "Flowchart: Process"),
    ("flowChartPunchedCard", "Flowchart: Card"),
    ("flowChartPunchedTape", "Flowchart: Punched Tape"),
    ("flowChartSort", "Flowchart: Sort"),
    ("flowChartSummingJunction", "Flowchart: Summing Junction"),
    ("flowChartTerminator", "Flowchart: Terminator"),
    ("foldedCorner", "Rectangle: Folded Corner"),
    ("frame", "Frame"),
    ("funnel", "Funnel"),
    ("gear6", "Gear 6"),
    ("gear9", "Gear 9"),
    ("halfFrame", "Half Frame"),
    ("heart", "Heart"),
    ("heptagon", "Heptagon"),
    ("hexagon", "Hexagon"),
    ("homePlate", "Arrow: Pentagon"),
    ("horizontalScroll", "Scroll: Horizontal"),
    ("irregularSeal1", "Explosion: 8 Points"),
    ("irregularSeal2", "Explosion: 14 Points"),
    ("leftArrow", "Arrow: Left"),
    ("leftArrowCallout", "Callout: Left Arrow"),
    ("leftBrace", "Left Brace"),
    ("leftBracket", "Left Bracket"),
    ("leftCircularArrow", "Arrow: Circular Left"),
    ("leftRightArrow", "Arrow: Left-Right"),
    ("leftRightArrowCallout", "Callout: Left-Right Arrow"),
    ("leftRightCircularArrow", "Arrow: Circular Left-Right"),
    ("leftRightRibbon", "Ribbon: Left-Right"),
    ("leftRightUpArrow", "Arrow: Left-Right-Up"),
    ("leftUpArrow", "Arrow: Left-Up"),
    ("lightningBolt", "Lightning Bolt"),
    ("line", "Straight Connector"),
    ("lineInv", "Straight Connector"),
    ("mathDivide", "Division Sign"),
    ("mathEqual", "Equals"),
    ("mathMinus", "Minus Sign"),
    ("mathMultiply", "Multiplication Sign"),
    ("mathNotEqual", "Not Equal"),
    ("mathPlus", "Plus Sign"),
    ("moon", "Moon"),
    ("noSmoking", "\"No\" Symbol"),
    ("nonIsoscelesTrapezoid", "Non-isosceles Trapezoid"),
    ("notchedRightArrow", "Arrow: Notched Right"),
    ("octagon", "Octagon"),
    ("parallelogram", "Parallelogram"),
    ("pentagon", "Pentagon"),
    ("pie", "Partial Circle"),
    ("pieWedge", "Pie Wedge"),
    ("plaque", "Plaque"),
    ("plaqueTabs", "Plaque Tabs"),
    ("plus", "Cross"),
    ("quadArrow", "Arrow: Quad"),
    ("quadArrowCallout", "Callout: Quad Arrow"),
    ("rect", "Rectangle"),
    ("ribbon", "Ribbon: Tilted Down"),
    ("ribbon2", "Ribbon: Tilted Up"),
    ("rightArrow", "Arrow: Right"),
    ("rightArrowCallout", "Callout: Right Arrow"),
    ("rightBrace", "Right Brace"),
    ("rightBracket", "Right Bracket"),
    ("round1Rect", "Rectangle: Single Corner Rounded"),
    ("round2DiagRect", "Rectangle: Diagonal Corners Rounded"),
    ("round2SameRect", "Rectangle: Top Corners Rounded"),
    ("roundRect", "Rectangle: Rounded Corners"),
    ("rtTriangle", "Right Triangle"),
    ("smileyFace", "Smiley Face"),
    ("snip1Rect", "Rectangle: Single Corner Snipped"),
    ("snip2DiagRect", "Rectangle: Diagonal Corners Snipped"),
    ("snip2SameRect", "Rectangle: Top Corners Snipped"),
    (
        "snipRoundRect",
        "Rectangle: Top Corners One Rounded and One Snipped",
    ),
    ("squareTabs", "Square Tabs"),
    ("star10", "Star: 10 Points"),
    ("star12", "Star: 12 Points"),
    ("star16", "Star: 16 Points"),
    ("star24", "Star: 24 Points"),
    ("star32", "Star: 32 Points"),
    ("star4", "Star: 4 Points"),
    ("star5", "Star: 5 Points"),
    ("star6", "Star: 6 Points"),
    ("star7", "Star: 7 Points"),
    ("star8", "Star: 8 Points"),
    ("straightConnector1", "Straight Arrow Connector"),
    ("stripedRightArrow", "Arrow: Striped Right"),
    ("sun", "Sun"),
    ("swooshArrow", "Swoosh Arrow"),
    ("teardrop", "Teardrop"),
    ("trapezoid", "Trapezoid"),
    ("triangle", "Isosceles Triangle"),
    ("upArrow", "Arrow: Up"),
    ("upArrowCallout", "Callout: Up Arrow"),
    ("upDownArrow", "Arrow: Up-Down"),
    ("upDownArrowCallout", "Callout: Up-Down Arrow"),
    ("uturnArrow", "Arrow: U-Turn"),
    ("verticalScroll", "Scroll: Vertical"),
    ("wave", "Wave"),
    ("wedgeEllipseCallout", "Speech Bubble: Oval"),
    ("wedgeRectCallout", "Speech Bubble: Rectangle"),
    (
        "wedgeRoundRectCallout",
        "Speech Bubble: Rectangle with Corners Rounded",
    ),
];

/// PowerPoint's name of a preset shape; an unknown preset gets its words ("myShape" → "My Shape").
#[must_use]
pub fn preset_display_name(preset: &str) -> String {
    if let Ok(i) = NAMES.binary_search_by(|(p, _)| p.cmp(&preset)) {
        return NAMES[i].1.to_owned();
    }
    let mut out = String::new();
    for (i, c) in preset.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_a_powerpoint_name() {
        assert!(NAMES.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
        for p in kiln_ooxml::drawingml::presets::preset_names() {
            assert!(NAMES.iter().any(|(n, _)| *n == p), "{p}");
        }
        assert_eq!(
            preset_display_name("roundRect"),
            "Rectangle: Rounded Corners"
        );
        assert_eq!(
            preset_display_name("flowChartProcess"),
            "Flowchart: Process"
        );
        assert_eq!(preset_display_name("someShape"), "Some Shape");
    }
}

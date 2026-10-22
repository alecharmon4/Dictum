use crate::color::Color;
use harper_core::Span;
use harper_core::linting::{Lint, LintKind, Suggestion};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Constructs a copy of the Rect, with the given amount of padding around the original.
    pub fn with_padding(&self, n: f64) -> Self {
        Self {
            x: self.x - n,
            y: self.y - n,
            width: self.width + 2.0 * n,
            height: self.height + 2.0 * n,
        }
    }

    /// Similar to [`Self::with_padding`], but preconfigured to a value that is forgiving to the
    /// user.
    pub fn with_friendly_padding(&self) -> Self {
        self.with_padding(2.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColoredRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: Color,
}

/// A Harper lint paired with the screen-space rectangle where it should be rendered.
///
/// Harper owns the lint details and accessibility owns the geometry; this type keeps both pieces
/// together without duplicating lint fields into highlighter-specific strings.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedLint {
    pub rect: Rect,
    pub lint: Lint,
}

/// Writes one edit, expressed against the field's text snapshot, back into the text field.
///
/// The OS broker creates one handle per text field and shares it between every lint found in that
/// field. Sharing is what lets the highlighter group lints by field for "Fix all".
pub type ApplyEdit = Rc<dyn Fn(Span<char>, Suggestion)>;

/// A Harper lint paired with geometry, source context, and an OS-specific edit action.
///
/// The highlighter owns rendering and interaction, but the OS broker is the only layer that knows how
/// to mutate the underlying text element. Storing the broker's edit handle here keeps those
/// responsibilities connected without teaching rendering code about Accessibility APIs.
pub struct ActionableLint {
    pub rect: Rect,
    pub rule_name: String,
    pub lint: Lint,
    pub source_text: String,
    apply_edit: ApplyEdit,
    /// Set once an edit built from this lint's snapshot has been written, so a stale lint can't
    /// write again before the next accessibility refresh replaces it.
    applied: Cell<bool>,
}

impl PositionedLint {
    pub fn new(rect: Rect, lint: Lint) -> Self {
        Self { rect, lint }
    }
}

impl ActionableLint {
    pub fn new(
        rect: Rect,
        rule_name: String,
        lint: Lint,
        source_text: String,
        apply_edit: ApplyEdit,
    ) -> Self {
        Self {
            rect,
            rule_name,
            lint,
            source_text,
            apply_edit,
            applied: Cell::new(false),
        }
    }

    pub fn apply_suggestion(&self, suggestion: Suggestion) {
        if !self.applied.replace(true) {
            (self.apply_edit)(self.lint.span, suggestion);
        }
    }

    /// Whether both lints came from the same text field.
    pub fn shares_field_with(&self, other: &ActionableLint) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.apply_edit), Rc::as_ptr(&other.apply_edit))
    }

    /// The top-suggestion edits "Fix all" would apply to this lint's field.
    pub fn fix_all_edits(&self, lints: &[ActionableLint]) -> Vec<(Span<char>, Suggestion)> {
        let mut edits: Vec<_> = lints
            .iter()
            .filter(|other| self.shares_field_with(other) && is_fix_all_kind(other.lint.lint_kind))
            .filter_map(|other| {
                let suggestion = other.lint.suggestions.first()?;
                Some((other.lint.span, suggestion.clone()))
            })
            .collect();

        // One lint can span several rectangles (e.g. across a line wrap), each its own entry.
        edits.sort_by_key(|(span, _)| span.start);
        edits.dedup_by(|a, b| a.0 == b.0);
        edits
    }

    /// Applies every "Fix all" edit in this lint's field as a single whole-field edit.
    pub fn fix_all(&self, lints: &[ActionableLint]) {
        let edits = self.fix_all_edits(lints);
        if edits.is_empty() || self.applied.replace(true) {
            return;
        }

        for other in lints.iter().filter(|other| self.shares_field_with(other)) {
            other.applied.set(true);
        }

        let fixed = combine_edits(&self.source_text, edits);
        let whole_field = Span::new(0, self.source_text.chars().count());
        (self.apply_edit)(whole_field, Suggestion::ReplaceWith(fixed));
    }
}

/// Lint kinds that are clear-cut corrections, as opposed to style advice the user should review
/// one at a time.
pub fn is_fix_all_kind(lint_kind: LintKind) -> bool {
    match lint_kind {
        LintKind::Agreement
        | LintKind::BoundaryError
        | LintKind::Capitalization
        | LintKind::Eggcorn
        | LintKind::Formatting
        | LintKind::Grammar
        | LintKind::Malapropism
        | LintKind::Punctuation
        | LintKind::Repetition
        | LintKind::Spelling
        | LintKind::Typo => true,
        LintKind::Enhancement
        | LintKind::Miscellaneous
        | LintKind::Nonstandard
        | LintKind::Readability
        | LintKind::Redundancy
        | LintKind::Regionalism
        | LintKind::Style
        | LintKind::Usage
        | LintKind::WordChoice
        | LintKind::WordOrder => false,
    }
}

/// Applies `edits` to `source` back to front, so earlier spans stay valid, and skips any edit that
/// overlaps one already applied.
pub fn combine_edits(source: &str, mut edits: Vec<(Span<char>, Suggestion)>) -> Vec<char> {
    let mut chars = source.chars().collect::<Vec<_>>();
    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start).then(b.0.end.cmp(&a.0.end)));

    let mut applied_from = usize::MAX;
    for (span, suggestion) in edits {
        if span.end > applied_from || span.end > chars.len() {
            continue;
        }
        suggestion.apply(span, &mut chars);
        applied_from = span.start;
    }

    chars
}

impl ColoredRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64, color: Color) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replace(start: usize, end: usize, with: &str) -> (Span<char>, Suggestion) {
        (
            Span::new(start, end),
            Suggestion::ReplaceWith(with.chars().collect()),
        )
    }

    fn combined(source: &str, edits: Vec<(Span<char>, Suggestion)>) -> String {
        combine_edits(source, edits).into_iter().collect()
    }

    #[test]
    fn combines_edits_regardless_of_input_order() {
        // "teh cat sat on teh mat" -> fix both "teh"s, given front-to-back.
        let edits = vec![replace(0, 3, "the"), replace(15, 18, "the")];
        assert_eq!(
            combined("teh cat sat on teh mat", edits),
            "the cat sat on the mat"
        );
    }

    #[test]
    fn length_changing_edits_keep_later_spans_valid() {
        let edits = vec![replace(0, 2, "I am"), replace(8, 12, "there")];
        assert_eq!(combined("Im over thre", edits), "I am over there");
    }

    #[test]
    fn overlapping_edit_is_skipped() {
        let edits = vec![replace(0, 5, "Hello"), replace(3, 8, "XXXXX")];
        assert_eq!(combined("Helol world", edits), "HelXXXXXrld");
    }

    #[test]
    fn remove_and_insert_suggestions_apply() {
        let edits = vec![
            (Span::new(4, 8), Suggestion::Remove),
            (Span::new(15, 18), Suggestion::InsertAfter(vec!['.'])),
        ];
        assert_eq!(combined("the the cat is big", edits), "the cat is big.");
    }

    #[test]
    fn out_of_range_edit_is_ignored() {
        let edits = vec![replace(20, 25, "nope")];
        assert_eq!(combined("short", edits), "short");
    }

    #[test]
    fn spelling_and_grammar_are_fix_all_kinds_but_style_is_not() {
        assert!(is_fix_all_kind(LintKind::Spelling));
        assert!(is_fix_all_kind(LintKind::Grammar));
        assert!(!is_fix_all_kind(LintKind::Style));
        assert!(!is_fix_all_kind(LintKind::WordChoice));
    }
}

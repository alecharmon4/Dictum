//! Replacements: user rules that fix words dictation gets wrong.
//!
//! Unlike custom words (fuzzy matching against a word list), a replacement is explicit: any of
//! its variants becomes the replacement text, optionally only when the dictation mentions one of
//! its context keywords. Rules run last, so they get the final say over the transcript.

use crate::settings::{self, Replacement};
use regex::RegexBuilder;
use tauri::AppHandle;

/// Applies every enabled rule to `text`, in order.
pub fn apply_replacements(text: &str, rules: &[Replacement]) -> String {
    let mut text = text.to_string();

    for rule in rules.iter().filter(|rule| rule.enabled) {
        if !mentions_any(&text, &rule.context_keywords) {
            continue;
        }

        // Longest first, so "lisp her" is replaced before "lisp" can split it.
        let mut variants = trimmed(&rule.variants);
        variants.sort_by_key(|variant| std::cmp::Reverse(variant.chars().count()));

        for variant in variants {
            text = replace_whole_words(&text, variant, &rule.replacement, rule.case_sensitive);
        }
    }

    text
}

fn trimmed(values: &[String]) -> Vec<&str> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect()
}

/// Whether `text` contains any of `keywords` as a whole word (always true when there are none).
fn mentions_any(text: &str, keywords: &[String]) -> bool {
    let keywords = trimmed(keywords);
    keywords.is_empty()
        || keywords
            .iter()
            .any(|keyword| !whole_word_matches(text, keyword, false).is_empty())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte ranges where `needle` occurs in `text` without touching a letter, digit, or underscore
/// on either side, so "AI" doesn't match inside "said".
fn whole_word_matches(text: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let Ok(pattern) = RegexBuilder::new(&regex::escape(needle))
        .case_insensitive(!case_sensitive)
        .build()
    else {
        return Vec::new();
    };

    pattern
        .find_iter(text)
        .filter(|found| {
            let before = text[..found.start()].chars().next_back();
            let after = text[found.end()..].chars().next();
            !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
        })
        .map(|found| (found.start(), found.end()))
        .collect()
}

fn replace_whole_words(
    text: &str,
    needle: &str,
    replacement: &str,
    case_sensitive: bool,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied_up_to = 0;

    for (start, end) in whole_word_matches(text, needle, case_sensitive) {
        result.push_str(&text[copied_up_to..start]);
        result.push_str(replacement);
        copied_up_to = end;
    }

    result.push_str(&text[copied_up_to..]);
    result
}

#[tauri::command]
#[specta::specta]
pub fn update_replacements(app: AppHandle, replacements: Vec<Replacement>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.replacements = replacements;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Shows what `text` would look like after `replacements`, for trying rules out before saving.
#[tauri::command]
#[specta::specta]
pub fn preview_replacements(text: String, replacements: Vec<Replacement>) -> String {
    apply_replacements(&text, &replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(variants: &[&str], replacement: &str) -> Replacement {
        Replacement {
            id: replacement.to_string(),
            variants: variants.iter().map(|v| v.to_string()).collect(),
            replacement: replacement.to_string(),
            context_keywords: Vec::new(),
            case_sensitive: false,
            enabled: true,
        }
    }

    #[test]
    fn replaces_every_variant_case_insensitively() {
        let rules = [rule(&["lisper", "lisp her"], "Lispr")];
        assert_eq!(
            apply_replacements("I use Lisper, well lisp her, daily", &rules),
            "I use Lispr, well Lispr, daily"
        );
    }

    #[test]
    fn only_matches_whole_words() {
        let rules = [rule(&["cat"], "dog")];
        assert_eq!(
            apply_replacements("cat concatenate cat_name cat.", &rules),
            "dog concatenate cat_name dog."
        );
    }

    #[test]
    fn variants_with_punctuation_still_match() {
        let rules = [rule(&["acme, inc."], "Acme, Inc.")];
        assert_eq!(
            apply_replacements("We bought acme, inc. today", &rules),
            "We bought Acme, Inc. today"
        );
    }

    #[test]
    fn longer_variants_win_over_their_prefixes() {
        let rules = [rule(&["lisp", "lisp her"], "Lispr")];
        assert_eq!(
            apply_replacements("open lisp her now", &rules),
            "open Lispr now"
        );
    }

    #[test]
    fn context_keywords_gate_the_rule() {
        let mut claude = rule(&["clawed"], "Claude");
        claude.context_keywords = vec!["AI".into(), "code".into()];
        let rules = [claude];

        assert_eq!(
            apply_replacements("I asked clawed to review the code", &rules),
            "I asked Claude to review the code"
        );
        assert_eq!(
            apply_replacements("The cat clawed the sofa", &rules),
            "The cat clawed the sofa"
        );
    }

    #[test]
    fn context_keywords_match_whole_words_only() {
        let mut claude = rule(&["clawed"], "Claude");
        claude.context_keywords = vec!["AI".into()];
        // "said" contains "ai" but doesn't mention AI.
        assert_eq!(
            apply_replacements("She said it clawed", &[claude]),
            "She said it clawed"
        );
    }

    #[test]
    fn case_sensitive_rules_ignore_other_casings() {
        let mut apple = rule(&["apple"], "Apple");
        apple.case_sensitive = true;
        assert_eq!(
            apply_replacements("apple pie at APPLE", &[apple]),
            "Apple pie at APPLE"
        );
    }

    #[test]
    fn disabled_and_empty_rules_do_nothing() {
        let mut off = rule(&["teh"], "the");
        off.enabled = false;
        let blank = rule(&["  ", ""], "x");
        assert_eq!(apply_replacements("teh end", &[off, blank]), "teh end");
    }

    #[test]
    fn replacement_text_is_literal() {
        let rules = [rule(&["price"], "$1 each")];
        assert_eq!(apply_replacements("the price", &rules), "the $1 each");
    }

    #[test]
    fn handles_non_ascii_text() {
        let rules = [rule(&["cafe"], "café")];
        // Accented letters count as word characters, so "écafe" is left alone.
        assert_eq!(
            apply_replacements("über cafe, écafe", &rules),
            "über café, écafe"
        );
    }
}

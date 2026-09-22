//! The place-and-date check every model-written text passes before it is shown
//! (constitution IV, F-301). A story naming a year, month or capitalised name absent from
//! the metadata is withheld. Mirrors `agents/src/mantelpiece_agents/narrative.py`.

use std::collections::BTreeSet;

pub const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const ORDINARY: [&str; 8] = ["day", "days", "the", "we", "our", "a", "an", "it"];

/// What the metadata allows a text to mention.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Allowed {
    pub years: BTreeSet<i32>,
    /// 1 to 12.
    pub months: BTreeSet<u8>,
    /// Lowercase names and phrases: places, people, the trip name.
    pub names: BTreeSet<String>,
}

impl Allowed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn date(&mut self, year: i32, month: u8) -> &mut Self {
        self.years.insert(year);
        self.months.insert(month);
        self
    }

    pub fn name(&mut self, name: &str) -> &mut Self {
        let n = name.trim().to_lowercase();
        if !n.is_empty() {
            self.names.insert(n);
        }
        self
    }

    fn words(&self) -> BTreeSet<String> {
        let mut w: BTreeSet<String> = self
            .names
            .iter()
            // "Stephan G. Stephansson": words are compared without their punctuation.
            .flat_map(|n| n.split_whitespace().map(bare).map(str::to_owned))
            .filter(|w| !w.is_empty())
            .collect();
        w.extend(
            self.months
                .iter()
                .map(|m| MONTHS[usize::from(*m) - 1].to_owned()),
        );
        w.extend(WEEKDAYS.iter().map(|d| (*d).to_owned()));
        w.extend(ORDINARY.iter().map(|d| (*d).to_owned()));
        w
    }
}

/// Problems found in `text`; empty means it stays within the facts.
pub fn check(text: &str, allowed: &Allowed) -> Vec<String> {
    let mut problems = Vec::new();
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    for t in &tokens {
        if t.len() == 4 && (t.starts_with("19") || t.starts_with("20")) {
            if let Ok(y) = t.parse::<i32>() {
                if !allowed.years.contains(&y) {
                    problems.push(format!("year {y} not in trip dates"));
                }
            }
        }
    }
    for (i, m) in MONTHS.iter().enumerate() {
        let month = u8::try_from(i + 1).unwrap_or(1);
        if tokens.contains(m) && !allowed.months.contains(&month) {
            problems.push(format!("month {m} not in trip dates"));
        }
    }
    let words = allowed.words();
    for phrase in capitalised_phrases(text) {
        let l = phrase.to_lowercase();
        if allowed.names.contains(&l)
            || l.split_whitespace()
                .all(|w| words.contains(w) || derived(w, &words))
        {
            continue;
        }
        problems.push(format!("unlisted name '{phrase}'"));
    }
    problems
}

/// Problems in `text` when `source` is all it may draw on (landmark trivia against its
/// encyclopedia summary): every number and every name must come from the source.
pub fn check_against_source(text: &str, source: &str) -> Vec<String> {
    let lower = source.to_lowercase();
    let source_tokens: BTreeSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let mut allowed = Allowed::new();
    allowed.name(source);
    // Dates count as numbers here: a year or month is fine exactly when the source has it.
    for t in &source_tokens {
        if let Ok(y) = t.parse::<i32>() {
            allowed.years.insert(y);
        }
        if let Some(i) = MONTHS.iter().position(|m| m == t) {
            allowed.months.insert(u8::try_from(i + 1).unwrap_or(1));
        }
    }
    let mut problems = check(text, &allowed);
    let text_lower = text.to_lowercase();
    for t in text_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
    {
        let problem = format!("number {t} not in the source");
        if !source_tokens.contains(t) && !problems.iter().any(|p| p.contains(t)) {
            problems.push(problem);
        }
    }
    problems
}

/// "icelandic" from "iceland", "scottish" from "scotland": a form of an allowed name, not a
/// new one. The shared opening must be at least four letters and most of the shorter word.
fn derived(word: &str, known: &BTreeSet<String>) -> bool {
    known.iter().any(|k| {
        let shared = word
            .chars()
            .zip(k.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let shorter = word.chars().count().min(k.chars().count());
        shared >= 4 && shared * 10 >= shorter * 6
    })
}

/// A word without the punctuation around it; apostrophes and hyphens inside stay.
fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}

/// Whether the next word opens a sentence: this one closes with `.`, `!`, `?` or `:`,
/// possibly inside quotes or brackets. An initial ("G.") closes nothing.
fn ends_sentence(raw: &str) -> bool {
    let closed = raw.trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}']);
    let initial = bare(closed).chars().count() == 1 && closed.ends_with('.');
    closed.ends_with(['.', '!', '?', ':']) && !initial
}

/// Runs of capitalised words that do not start a sentence.
fn capitalised_phrases(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let word = bare(raw);
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        if capitalised && !sentence_start {
            current.push(word);
        } else if !current.is_empty() {
            out.push(current.join(" "));
            current.clear();
        }
        // A phrase never runs across the end of a sentence.
        if ends_sentence(raw) && !current.is_empty() {
            out.push(current.join(" "));
            current.clear();
        }
        sentence_start = ends_sentence(raw);
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kyoto() -> Allowed {
        let mut a = Allowed::new();
        a.date(2026, 4)
            .name("Kinkaku-ji")
            .name("Fushimi Inari")
            .name("Hanyu")
            .name("Maya")
            .name("Kyoto, April 2026");
        a
    }

    #[test]
    fn stays_within_the_facts() {
        let text = "Hanyu started at Kinkaku-ji. Maya arrived for Fushimi Inari on Saturday. The last day was quiet.";
        assert!(check(text, &kyoto()).is_empty());
    }

    #[test]
    fn flags_unlisted_year_month_and_name() {
        let problems = check("Grandma flew in from Osaka in May 2025.", &kyoto());
        assert!(problems.iter().any(|p| p.contains("2025")), "{problems:?}");
        assert!(
            problems.iter().any(|p| p.contains("month may")),
            "{problems:?}"
        );
        assert!(problems.iter().any(|p| p.contains("Osaka")), "{problems:?}");
        assert!(check("Kyoto in April 2026 was gentle.", &kyoto()).is_empty());
    }

    #[test]
    fn a_form_of_an_allowed_name_is_that_name() {
        let mut allowed = Allowed::default();
        allowed.name("Iceland");
        allowed.date(2015, 6);
        assert!(check("They loved the Icelandic light.", &allowed).is_empty());
        assert_eq!(
            check("They loved the Irish light.", &allowed),
            vec!["unlisted name 'Irish'"]
        );
    }

    #[test]
    fn paragraph_openers_quotes_and_initials_are_handled() {
        let mut allowed = Allowed::default();
        allowed
            .name("Stephan G. Stephansson Monument")
            .name("Iceland")
            .name("Hanyu");
        allowed.date(2015, 6);
        allowed.date(2015, 7);
        let text = "On June 27th they reached Iceland.\n\nOn June 28th Hanyu stood by the Stephan G. Stephansson Monument. \"What a view!\" On July 1st they drove on.";
        assert_eq!(check(text, &allowed), Vec::<String>::new());
        assert_eq!(
            check(
                "On June 27th they reached Iceland. Then Paris called.",
                &allowed
            ),
            vec!["unlisted name 'Paris'"]
        );
    }
}

//! The place-and-date check every model-written text passes before it is shown
//! (constitution IV, F-301). A story naming a year, month or capitalised name absent from
//! the metadata is withheld. Mirrors `agents/src/memorylane_agents/narrative.py`.

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
            .flat_map(|n| n.split_whitespace().map(str::to_owned))
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
        if allowed.names.contains(&l) || l.split_whitespace().all(|w| words.contains(w)) {
            continue;
        }
        problems.push(format!("unlisted name '{phrase}'"));
    }
    problems
}

/// Runs of capitalised words that do not start a sentence.
fn capitalised_phrases(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-');
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        if capitalised && !sentence_start {
            current.push(word);
        } else {
            if !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            }
            if capitalised && sentence_start && raw.ends_with(['.', '!', '?']) {
                // A one-word sentence; nothing to check.
            }
        }
        sentence_start = raw.ends_with(['.', '!', '?']);
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
}

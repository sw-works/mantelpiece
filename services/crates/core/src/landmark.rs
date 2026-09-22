//! What is known about a landmark (F-210): an encyclopedia summary fetched once and shared
//! by every household, and a few short trivia lines written from it. The summary is the
//! only source; a trivia line that says more than the summary does is refused.

use crate::geo::GeoPoint;
use crate::narrative_check::check_against_source;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const MAX_TRIVIA: usize = 3;
pub const MAX_TRIVIA_CHARS: usize = 160;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandmarkFacts {
    /// `key(name)`: how photos and routes find this record.
    pub key: String,
    /// The name as the photo or route carries it.
    pub name: String,
    /// The article's own title.
    pub title: String,
    /// One-line description, when the source has one ("waterfall in Iceland").
    #[serde(default)]
    pub tagline: Option<String>,
    pub summary: String,
    pub url: String,
    /// Who to credit on screen: "Wikipedia, CC BY-SA 4.0".
    pub credit: String,
    #[serde(default)]
    pub at: Option<GeoPoint>,
    #[serde(default)]
    pub trivia: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

/// Lowercase letters and digits joined by `-`: "Skógafoss Waterfall" and "skógafoss
/// waterfall." share a key.
pub fn key(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

impl LandmarkFacts {
    /// What a viewer reads when nobody has written trivia yet: the summary's opening
    /// sentence or two, within `max_chars`.
    pub fn lead(&self, max_chars: usize) -> String {
        let mut out = String::new();
        for sentence in sentences(&self.summary) {
            if !out.is_empty() && out.chars().count() + sentence.chars().count() + 1 > max_chars {
                break;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(sentence);
            if out.chars().count() >= max_chars / 2 {
                break;
            }
        }
        if out.chars().count() > max_chars {
            let cut: String = out.chars().take(max_chars.saturating_sub(1)).collect();
            out = format!("{}…", cut.trim_end());
        }
        out
    }

    /// The lines shown with a photo: trivia when there is some, else the lead.
    pub fn lines(&self) -> Vec<String> {
        if self.trivia.is_empty() {
            let lead = self.lead(220);
            if lead.is_empty() {
                vec![]
            } else {
                vec![lead]
            }
        } else {
            self.trivia.clone()
        }
    }

    /// Keep trivia only if every line stays inside the summary. Returns the problems;
    /// empty means the lines were stored on `self`.
    pub fn set_trivia(&mut self, lines: &[String]) -> Vec<String> {
        let lines: Vec<String> = lines
            .iter()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect();
        let mut problems = Vec::new();
        if lines.is_empty() {
            problems.push("no trivia lines".to_owned());
        }
        if lines.len() > MAX_TRIVIA {
            problems.push(format!("at most {MAX_TRIVIA} lines"));
        }
        let source = format!(
            "{} {} {} {}",
            self.name,
            self.title,
            self.tagline.as_deref().unwrap_or(""),
            self.summary
        );
        for line in &lines {
            if line.chars().count() > MAX_TRIVIA_CHARS {
                problems.push(format!("line over {MAX_TRIVIA_CHARS} characters"));
            }
            problems.extend(check_against_source(line, &source));
        }
        if problems.is_empty() {
            self.trivia = lines;
        }
        problems
    }
}

/// Sentences of `text`, split on `. ` after a lowercase letter, digit or bracket, so
/// "St. Paul" and "G. Stephansson" stay whole.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    for w in bytes.windows(3) {
        let [(_, before), (i, dot), (_, after)] = [w[0], w[1], w[2]];
        if matches!(dot, '.' | '!' | '?')
            && after == ' '
            && (before.is_lowercase() || before.is_ascii_digit() || before == ')')
        {
            out.push(text[start..=i].trim());
            start = i + 1;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn skogafoss() -> LandmarkFacts {
        LandmarkFacts {
            key: key("Skógafoss"),
            name: "Skógafoss".into(),
            title: "Skógafoss".into(),
            tagline: Some("waterfall in Iceland".into()),
            summary: "Skógafoss is a waterfall on the Skógá River in the south of Iceland at the cliff marking the former coastline. After the coastline had receded (it is now at a distance of about 5 kilometres from Skógar), the former sea cliffs remained. The waterfall is 25 metres wide and has a drop of 60 metres.".into(),
            url: "https://en.wikipedia.org/wiki/Sk%C3%B3gafoss".into(),
            credit: "Wikipedia, CC BY-SA 4.0".into(),
            at: None,
            trivia: vec![],
            fetched_at: datetime!(2026-09-21 10:00 UTC),
        }
    }

    #[test]
    fn names_that_differ_in_case_and_punctuation_share_a_key() {
        assert_eq!(key("Skógafoss Waterfall."), "skógafoss-waterfall");
        assert_eq!(key("  St. Paul's  Cathedral"), "st-paul-s-cathedral");
    }

    #[test]
    fn the_lead_is_whole_sentences_and_stands_in_for_trivia() {
        let f = skogafoss();
        let lead = f.lead(220);
        assert!(lead.starts_with("Skógafoss is a waterfall"), "{lead}");
        assert!(lead.ends_with("coastline."), "{lead}");
        assert_eq!(f.lines(), vec![lead]);
    }

    #[test]
    fn trivia_is_kept_only_when_the_summary_backs_it() {
        let mut f = skogafoss();
        let good = vec![
            "The drop is 60 metres, and the fall is 25 metres wide.".to_owned(),
            "These cliffs were once the coastline of Iceland.".to_owned(),
        ];
        assert_eq!(f.set_trivia(&good), Vec::<String>::new());
        assert_eq!(f.lines(), good);

        let mut f = skogafoss();
        let invented = vec!["Legend says Vikings hid gold here in 1002.".to_owned()];
        let problems = f.set_trivia(&invented);
        assert!(problems.iter().any(|p| p.contains("1002")), "{problems:?}");
        assert!(
            problems.iter().any(|p| p.contains("Vikings")),
            "{problems:?}"
        );
        assert!(f.trivia.is_empty());
    }
}

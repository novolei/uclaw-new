//! Does a retrieved text contain the expected fact? `expect_any`: lowercase
//! substring; `expect_word`: case-sensitive ASCII-word-bounded regex.

use super::GoldenItem;

pub struct Matcher {
    any: Vec<String>,
    words: Vec<regex::Regex>,
}

impl Matcher {
    pub fn new(item: &GoldenItem) -> Self {
        Self {
            any: item.expect_any.iter().map(|s| s.to_lowercase()).filter(|s| !s.is_empty()).collect(),
            words: item
                .expect_word
                .iter()
                .filter(|w| !w.is_empty())
                .filter_map(|w| regex::Regex::new(&format!(r"(?-u:\b){}(?-u:\b)", regex::escape(w))).ok())
                .collect(),
        }
    }

    pub fn hits(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.any.iter().any(|a| lower.contains(a.as_str())) || self.words.iter().any(|re| re.is_match(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(any: &[&str], word: &[&str]) -> GoldenItem {
        GoldenItem {
            id: "x".into(),
            lang: "en".into(),
            category: String::new(),
            query: "q".into(),
            expect_any: any.iter().map(|s| s.to_string()).collect(),
            expect_word: word.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn matcher_substring_is_case_insensitive_and_word_is_bounded() {
        let m = Matcher::new(&item(&["Hangzhou", "杭州"], &[]));
        assert!(m.hits("lives in HANGZHOU now"));
        assert!(m.hits("住在杭州"));
        assert!(!m.hits("lives in Shanghai"));
        let w = Matcher::new(&item(&[], &["AB"]));
        assert!(w.hits("call me AB please"));
        assert!(!w.hits("ABC is not it"), "word boundary");
        assert!(!w.hits("call me ab"), "case-sensitive");
        assert!(!Matcher::new(&item(&[], &[])).hits("anything"), "no expectations never hit");
    }
}

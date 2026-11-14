//! Small text helpers for messages shown in the GUI.
//!
//! Exists because the view models phrase errors as lowercase clauses (so they
//! can be embedded, e.g. `address: ...`), while the pages show them as
//! sentences.

/// Starts `text` with a capital letter.
pub fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalises_the_first_letter_only() {
        assert_eq!(capitalise("nothing to send"), "Nothing to send");
        assert_eq!(capitalise("ärger"), "Ärger");
        assert_eq!(capitalise("Already"), "Already");
        assert_eq!(capitalise(""), "");
    }
}

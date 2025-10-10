#[cfg(test)]
mod tests {
    use crate::tools::clean::{clean, clean_all};

    #[test]
    fn test_html_entities_named() {
        assert_eq!(clean("&amp;"), "&");
        assert_eq!(clean("&lt;"), "<");
        assert_eq!(clean("&gt;"), ">");
        assert_eq!(clean("&quot;"), "\"");
        assert_eq!(clean("&apos;"), "'");
        assert_eq!(clean("&nbsp;"), "\u{00A0}");
    }

    #[test]
    fn test_html_entities_numeric() {
        assert_eq!(clean("&#39;"), "'");
        assert_eq!(clean("&#x27;"), "'");
        assert_eq!(clean("&#34;"), "\"");
        assert_eq!(clean("&#x22;"), "\"");
    }

    #[test]
    fn test_html_entities_combined() {
        assert_eq!(clean("&lt;div&gt;"), "<div>");
        assert_eq!(clean("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(clean("It&#39;s &quot;great&quot;"), "It's \"great\"");
    }

    #[test]
    fn test_unicode_normalization() {
        // Precomposed vs decomposed characters
        let precomposed = "\u{00E9}"; // é (single character)
        let decomposed = "e\u{0301}"; // e + combining acute accent

        // After cleaning, both should be the same
        assert_eq!(clean(precomposed), clean(decomposed));
        assert_eq!(clean(precomposed), "é");
    }

    #[test]
    fn test_zero_width_characters() {
        assert_eq!(clean("hello\u{200B}world"), "hello world");
        assert_eq!(clean("test\u{200C}ing"), "testing");
        assert_eq!(clean("word\u{200D}join"), "wordjoin");
        assert_eq!(clean("\u{FEFF}text"), "text");
    }

    #[test]
    fn test_control_characters() {
        // Should remove most control characters
        assert_eq!(clean("hello\x00world"), "hello world");
        assert_eq!(clean("test\x01ing"), "testing");

        // But keep newlines and tabs
        assert_eq!(clean("line1\nline2"), "line1 line2"); // Normalized to space
        assert_eq!(clean("tab\there"), "tab here"); // Normalized to space
    }

    #[test]
    fn test_whitespace_normalization() {
        assert_eq!(clean("hello   world"), "hello world");
        assert_eq!(clean("  trim  me  "), "trim me");
        assert_eq!(clean("multiple\n\n\nlines"), "multiple lines");
        assert_eq!(clean("lots\t\t\tof\t\ttabs"), "lots of tabs");
        assert_eq!(clean("  leading and trailing  "), "leading and trailing");
    }

    #[test]
    fn test_combined_cleaning() {
        let dirty = "Hello &amp; &#39;world&#39;   with   \u{200B}spaces\x00";
        assert_eq!(clean(dirty), "Hello & 'world' with spaces");
    }

    #[test]
    fn test_real_world_examples() {
        // Recipe title with entities
        assert_eq!(
            clean("Tom&#39;s &amp; Jerry&#39;s Favorite Dish"),
            "Tom's & Jerry's Favorite Dish"
        );

        // Description with extra whitespace
        assert_eq!(
            clean("A   delicious   recipe   for   everyone!"),
            "A delicious recipe for everyone!"
        );

        // Mixed issues
        assert_eq!(
            clean("  &lt;b&gt;Bold&lt;/b&gt;   text  "),
            "<b>Bold</b> text"
        );
    }

    #[test]
    fn test_empty_and_whitespace() {
        assert_eq!(clean(""), "");
        assert_eq!(clean("   "), "");
        assert_eq!(clean("\n\n\n"), "");
        assert_eq!(clean("\t\t\t"), "");
    }

    #[test]
    fn test_no_changes_needed() {
        assert_eq!(clean("perfect text"), "perfect text");
        assert_eq!(clean("no entities here"), "no entities here");
    }

    #[test]
    fn test_clean_all() {
        let texts = vec![
            "Text &amp; stuff".to_string(),
            "More &#39;text&#39;".to_string(),
            "  spaced  out  ".to_string(),
        ];

        let cleaned = clean_all(&texts);

        assert_eq!(cleaned[0], "Text & stuff");
        assert_eq!(cleaned[1], "More 'text'");
        assert_eq!(cleaned[2], "spaced out");
    }

    #[test]
    fn test_preserves_intentional_characters() {
        // Should not remove these
        assert_eq!(clean("hello-world"), "hello-world");
        assert_eq!(clean("under_score"), "under_score");
        assert_eq!(clean("with.period"), "with.period");
        assert_eq!(clean("a/b/c"), "a/b/c");
    }

    #[test]
    fn test_unicode_characters() {
        // Emoji and other unicode should be preserved
        assert_eq!(clean("Hello 👋 World 🌍"), "Hello 👋 World 🌍");
        assert_eq!(clean("Café"), "Café");
        assert_eq!(clean("日本語"), "日本語");
    }
}

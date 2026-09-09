use unicode_segmentation::UnicodeSegmentation;
pub use unicode_width::UnicodeWidthChar;

pub trait DisplayWidth {
    fn width(&self) -> usize;
}

impl DisplayWidth for str {
    #[inline]
    fn width(&self) -> usize {
        if self.is_ascii() {
            return self.bytes().filter(|b| !b.is_ascii_control()).count();
        }
        // Cells are rendered independently, so ligatures cannot span graphemes.
        self.graphemes(true).map(grapheme_width).sum()
    }
}

#[inline]
pub fn grapheme_width(grapheme: &str) -> usize {
    if grapheme.len() == 1 {
        return usize::from(!grapheme.as_bytes()[0].is_ascii_control());
    }
    if grapheme.chars().next().is_some_and(char::is_control) {
        return 0;
    }
    unicode_width::UnicodeWidthStr::width(grapheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_sequences() {
        for (text, width) in [
            ("", 0),
            ("plain text", 10),
            ("a\nb\r\nc\td\0\u{7f}\u{85}", 4),
            ("لا", 2),
            ("א\u{200d}ל", 2),
            ("ꓹꓼ", 2),
            ("🤦🏼‍♂️", 2),
            ("👨‍👩‍👧‍👦", 2),
            ("1️⃣", 2),
            ("🇬🇧", 2),
            ("☀️", 2),
            ("☀︎", 1),
            ("e\u{301}", 1),
            ("コン", 4),
            ("\u{200b}", 0),
            ("\u{301}", 0),
            ("\u{600}a", 2),
            ("\u{17d8}", 3),
        ] {
            assert_eq!(text.width(), width, "{text:?}");
        }
    }

    #[test]
    fn document_graphemes_remain_editable() {
        for text in ["\n", "\r\n", "\0", "\u{200b}", "\u{301}"] {
            assert_eq!(text.width(), 0, "{text:?}");
            assert_eq!(crate::graphemes::grapheme_width(text), 1, "{text:?}");
        }
        for text in ["🤦🏼‍♂️", "👨‍👩‍👧‍👦", "1️⃣", "🇬🇧", "☀️"]
        {
            assert_eq!(crate::graphemes::grapheme_width(text), 2, "{text:?}");
        }
    }
}

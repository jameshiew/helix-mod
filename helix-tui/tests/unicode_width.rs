use helix_tui::buffer::Buffer;
use helix_tui::layout::Alignment;
use helix_tui::text::{Span, Spans, Text};
use helix_tui::widgets::{Paragraph, Widget, Wrap};
use helix_view::graphics::{Rect, Style};

#[test]
fn fitted_buffers_preserve_ligature_graphemes() {
    for (text, first, second) in [
        ("لا", "ل", "ا"),
        ("א\u{200d}ל", "א\u{200d}", "ל"),
        ("ꓹꓼ", "ꓹ", "ꓼ"),
    ] {
        let buffer = Buffer::with_lines(vec![text]);
        assert_eq!(buffer.area.width, 2, "{text:?}");
        assert_eq!(buffer[(0, 0)].symbol.as_str(), first);
        assert_eq!(buffer[(1, 0)].symbol.as_str(), second);
        assert_eq!(Span::raw(text).width(), 2);

        let mut truncated = Buffer::empty(Rect::new(0, 0, 3, 1));
        truncated.set_string_truncated(0, 0, text, 3, |_| Style::default(), false, true);
        assert_eq!(truncated[(0, 0)].symbol.as_str(), first);
        assert_eq!(truncated[(1, 0)].symbol.as_str(), second);
    }
}

#[test]
fn inline_buffers_skip_controls() {
    for text in ["a\nb", "a\r\nb", "a\tb", "a\0b", "a\u{85}b"] {
        let buffer = Buffer::with_lines(vec![text]);
        assert_eq!(buffer.area.width, 2, "{text:?}");
        assert_eq!(buffer[(0, 0)].symbol.as_str(), "a");
        assert_eq!(buffer[(1, 0)].symbol.as_str(), "b");
    }
}

#[test]
fn emoji_cells_and_diff_use_two_columns() {
    for emoji in ["🤦🏼‍♂️", "👨‍👩‍👧‍👦", "1️⃣", "🇬🇧", "☀️"]
    {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
        assert_eq!(
            buffer.set_stringn(0, 0, &format!("a{emoji}b"), 4, Style::default()),
            (4, 0)
        );
        assert_eq!(buffer[(1, 0)].symbol.as_str(), emoji);
        assert_eq!(buffer[(1, 0)].width(), 2);
        assert_eq!(buffer[(2, 0)].symbol.as_str(), " ");
        assert_eq!(buffer[(3, 0)].symbol.as_str(), "b");

        let empty = Buffer::empty(buffer.area);
        let drawn: Vec<_> = empty.diff(&buffer).iter().map(|(x, _, _)| *x).collect();
        assert_eq!(drawn, [0, 1, 3]);
        let erased: Vec<_> = buffer.diff(&empty).iter().map(|(x, _, _)| *x).collect();
        assert_eq!(erased, [0, 1, 2, 3]);
    }
}

#[test]
fn redraw_skips_all_three_columns_of_a_khmer_sign() {
    let previous = Buffer::with_lines(vec!["abc!"]);
    let current = Buffer::with_lines(vec!["\u{17d8}!"]);
    assert_eq!(current.area.width, 4);
    assert_eq!(current[(0, 0)].width(), 3);
    let drawn: Vec<_> = previous.diff(&current).iter().map(|(x, _, _)| *x).collect();
    assert_eq!(drawn, [0]);
    let erased: Vec<_> = current.diff(&previous).iter().map(|(x, _, _)| *x).collect();
    assert_eq!(erased, [0, 1, 2]);
}

#[test]
fn truncation_keeps_wide_graphemes_inside_the_row() {
    for text in ["1️⃣b", "🤦🏼‍♂️b", "\u{17d8}b"] {
        for truncate_start in [false, true] {
            for ellipsis in [false, true] {
                let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
                buffer.set_string_truncated(
                    0,
                    0,
                    text,
                    1,
                    |_| Style::default(),
                    ellipsis,
                    truncate_start,
                );
                assert_eq!(
                    buffer[(0, 0)].symbol.as_str(),
                    if ellipsis {
                        "…"
                    } else if truncate_start {
                        "b"
                    } else {
                        " "
                    }
                );
            }
        }
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer.set_spans_truncated(0, 0, &Spans::from(Span::raw(text)), 1);
        assert_eq!(buffer[(0, 0)].symbol.as_str(), "…");
        buffer.set_string_anchored(0, 0, true, false, text, 1, |_| Style::default());
        assert_eq!(buffer[(0, 0)].symbol.as_str(), "…");
    }
}

#[test]
fn truncation_stops_at_a_wide_grapheme_between_spans() {
    let spans = Spans::from(vec![Span::raw("a"), Span::raw("\u{17d8}b")]);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 1));
    buffer.set_spans_truncated(0, 0, &spans, 3);
    assert_eq!(buffer[(0, 0)].symbol.as_str(), "…");
    assert_eq!(buffer[(1, 0)].symbol.as_str(), " ");
    assert_eq!(buffer[(2, 0)].symbol.as_str(), "b");
}

#[test]
fn oversized_widths_do_not_write_into_the_next_row() {
    for width in [2, usize::MAX] {
        for truncate_start in [false, true] {
            let mut buffer = Buffer::with_lines(vec!["ab", "cd"]);
            buffer.set_string_anchored(1, 0, truncate_start, false, "1️⃣x", width, |_| {
                Style::default()
            });
            assert_eq!(buffer[(0, 1)].symbol.as_str(), "c");
            assert_eq!(buffer[(1, 1)].symbol.as_str(), "d");
            buffer.set_string_truncated(
                1,
                0,
                "1️⃣x",
                width,
                |_| Style::default(),
                true,
                truncate_start,
            );
            assert_eq!(buffer[(0, 1)].symbol.as_str(), "c");
            assert_eq!(buffer[(1, 1)].symbol.as_str(), "d");
            buffer.set_spans_truncated(1, 0, &Spans::from(Span::raw("1️⃣x")), u16::MAX);
            assert_eq!(buffer[(0, 1)].symbol.as_str(), "c");
            assert_eq!(buffer[(1, 1)].symbol.as_str(), "d");
        }
    }
}

#[test]
fn paragraphs_align_and_wrap_graphemes() {
    let text = Text::from("لا\r\n1️⃣x");
    let area = Rect::new(0, 0, 4, 2);
    let mut buffer = Buffer::empty(area);
    let paragraph = Paragraph::new(&text).alignment(Alignment::Right);
    assert_eq!(paragraph.required_size(4), (3, 2));
    paragraph.render(area, &mut buffer);
    assert_eq!(buffer[(2, 0)].symbol.as_str(), "ل");
    assert_eq!(buffer[(3, 0)].symbol.as_str(), "ا");
    assert_eq!(buffer[(1, 1)].symbol.as_str(), "1️⃣");
    assert_eq!(buffer[(3, 1)].symbol.as_str(), "x");

    let text = Text::from("1️⃣ 1️⃣");
    let paragraph = Paragraph::new(&text).wrap(Wrap { trim: true });
    assert_eq!(paragraph.required_size(3), (2, 2));
    let area = Rect::new(0, 0, 3, 2);
    let mut buffer = Buffer::empty(area);
    paragraph.render(area, &mut buffer);
    assert_eq!(buffer[(0, 0)].symbol.as_str(), "1️⃣");
    assert_eq!(buffer[(0, 1)].symbol.as_str(), "1️⃣");
}

#[test]
fn paragraphs_skip_trailing_zero_width_graphemes() {
    for suffix in ["\u{200b}", "\u{605}", "\u{890}"] {
        let text = Text::from(format!("x{suffix}"));
        let area = Rect::new(0, 0, 1, 1);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(&text).render(area, &mut buffer);
        assert_eq!(buffer[(0, 0)].symbol.as_str(), "x");
    }
}

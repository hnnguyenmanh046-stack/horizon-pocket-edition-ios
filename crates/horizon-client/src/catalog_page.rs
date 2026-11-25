//! Horizon PE: a catalog project's description as lines of plain text.
//!
//! The sources write Markdown (the registry's `description`, Modrinth's body). The catalog's
//! screen on a phone draws text and nothing else, so this keeps the words and their shape
//! (headings, paragraphs, list items) and drops the marks, the pictures and the HTML.

/// One block of a description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Heading(String),
    Text(String),
    /// A list's item.
    Item(String),
    /// The space between two blocks.
    Gap,
}

/// `[label](target)` or `[label][reference]` whose `[` is at `open`: the label, and where the
/// text goes on after it.
fn link(chars: &[char], open: usize) -> Option<(String, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (i, c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let label: String = chars[open + 1..close].iter().collect();
    let (opening, closing) = match chars.get(close + 1)? {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        _ => return None,
    };
    let mut depth = 0;
    for (i, c) in chars.iter().enumerate().skip(close + 1) {
        if *c == opening {
            depth += 1;
        } else if *c == closing {
            depth -= 1;
            if depth == 0 {
                return Some((label, i + 1));
            }
        }
    }
    None
}

/// Where the HTML tag whose `<` is at `open` ends, if that is one (`<br>`, `</p>`, `<img ...>`).
fn tag_end(chars: &[char], open: usize) -> Option<usize> {
    let first = *chars.get(open + 1)?;
    if !(first.is_ascii_alphabetic() || first == '/' || first == '!') {
        return None;
    }
    let close = chars.iter().skip(open).position(|c| *c == '>')?;
    Some(open + close + 1)
}

/// The words of `text` without Markdown's marks: a link is its label, an image and a tag are
/// nothing, and the stars, backticks and doubled underscores of emphasis go.
fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '!'
            && chars.get(i + 1) == Some(&'[')
            && let Some((_, after)) = link(&chars, i + 1)
        {
            i = after;
            continue;
        }
        if c == '['
            && let Some((label, after)) = link(&chars, i)
        {
            out.push_str(&inline(&label));
            i = after;
            continue;
        }
        if c == '<'
            && let Some(after) = tag_end(&chars, i)
        {
            i = after;
            continue;
        }
        let doubled = chars.get(i + 1) == Some(&c) || (i > 0 && chars[i - 1] == c);
        match c {
            '*' | '`' => {}
            '_' | '~' if doubled => {}
            _ => out.push(c),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A heading's words, when `line` is one (`## Words`).
fn heading(line: &str) -> Option<&str> {
    let words = line.trim_start_matches('#');
    let marks = line.len() - words.len();
    ((1..=6).contains(&marks) && words.starts_with(' '))
        .then(|| words.trim().trim_end_matches('#').trim_end())
}

/// A list item's words, when `line` starts one (`- words`, `3. words`).
fn list_item(line: &str) -> Option<&str> {
    for mark in ["- ", "* ", "+ "] {
        if let Some(words) = line.strip_prefix(mark) {
            return Some(words.trim_start());
        }
    }
    let number = line.trim_start_matches(|c: char| c.is_ascii_digit());
    if number.len() == line.len() {
        return None;
    }
    number
        .strip_prefix(". ")
        .or_else(|| number.strip_prefix(") "))
        .map(str::trim_start)
}

/// A line that only draws a rule (`---`), or a table's line under its head (`|---|:--|`).
fn rule(line: &str) -> bool {
    let marks = line.chars().filter(|c| "-*_".contains(*c)).count();
    marks >= 3 && line.chars().all(|c| "-*_|: ".contains(c))
}

/// What is gathered of the block being read.
#[derive(Default)]
struct Block {
    text: String,
    item: bool,
}

fn gap(out: &mut Vec<Line>) {
    if out.last().is_some_and(|last| *last != Line::Gap) {
        out.push(Line::Gap);
    }
}

fn flush(block: &mut Block, out: &mut Vec<Line>) {
    let Block { text, item } = std::mem::take(block);
    let raw = text.trim();
    let words = inline(raw);
    if words.is_empty() {
        return;
    }
    // A paragraph that is one bold phrase is a heading in all but name.
    let bold = raw.len() > 4
        && raw.starts_with("**")
        && raw.ends_with("**")
        && !raw[2..raw.len() - 2].contains("**");
    out.push(if item {
        Line::Item(words)
    } else if bold {
        Line::Heading(words)
    } else {
        Line::Text(words)
    });
}

/// `markdown` as the blocks to draw, one after the other.
pub fn lines(markdown: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut block = Block::default();
    let mut fenced = false;
    for raw in markdown.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            flush(&mut block, &mut out);
            fenced = !fenced;
            continue;
        }
        if fenced {
            // Code is shown as it is written.
            if !line.is_empty() {
                out.push(Line::Text(raw.trim_end().to_owned()));
            }
            continue;
        }
        if line.is_empty() || rule(line) {
            flush(&mut block, &mut out);
            // The line under a table's head parts nothing.
            if !line.contains('|') {
                gap(&mut out);
            }
        } else if let Some(words) = heading(line) {
            flush(&mut block, &mut out);
            gap(&mut out);
            let words = inline(words);
            if !words.is_empty() {
                out.push(Line::Heading(words));
            }
        } else if let Some(words) = list_item(line) {
            flush(&mut block, &mut out);
            block = Block {
                text: words.to_owned(),
                item: true,
            };
        } else if line.starts_with('|') {
            // A table's row: its cells side by side.
            flush(&mut block, &mut out);
            let cells: Vec<String> = line
                .trim_matches('|')
                .split('|')
                .map(inline)
                .filter(|cell| !cell.is_empty())
                .collect();
            if !cells.is_empty() {
                out.push(Line::Text(cells.join(" \u{b7} ")));
            }
        } else {
            // The next line of the paragraph or the item being read.
            if !block.text.is_empty() {
                block.text.push(' ');
            }
            block.text.push_str(line.trim_start_matches('>').trim_start());
        }
    }
    flush(&mut block, &mut out);
    if out.last() == Some(&Line::Gap) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use Line::{Gap, Heading, Item, Text};

    fn text(words: &str) -> Line {
        Text(words.to_owned())
    }
    fn item(words: &str) -> Line {
        Item(words.to_owned())
    }
    fn head(words: &str) -> Line {
        Heading(words.to_owned())
    }

    #[test]
    fn a_registry_description_keeps_its_words_and_shape() {
        let found = lines(
            "A port of [Enhanced](https://m.com/mod/x) 1.8, made by\n\
             [CreativeMD](https://github.com/CreativeMD).\n\
             \n\
             **What it draws**\n\
             \n\
             - **Blood when hurt.** The damage picks\n  the splat.\n\
             - Second\n",
        );
        let wanted = [
            text("A port of Enhanced 1.8, made by CreativeMD."),
            Gap,
            head("What it draws"),
            Gap,
            item("Blood when hurt. The damage picks the splat."),
            item("Second"),
        ];
        assert_eq!(found, wanted);
    }

    #[test]
    fn headings_rules_and_numbered_lists() {
        let found = lines("# Title\n## Sub ##\n1. One\n2) Two\n\n---\nEnd\n\n");
        let wanted = [
            head("Title"),
            Gap,
            head("Sub"),
            item("One"),
            item("Two"),
            Gap,
            text("End"),
        ];
        assert_eq!(found, wanted);
    }

    #[test]
    fn pictures_and_html_are_dropped_and_the_words_stay() {
        let source = "![shot](a.png)\n\n<p align=\"center\">Hi <b>there</b></p> 2 < 3";
        let found = lines(source);
        assert_eq!(found, [text("Hi there 2 < 3")]);
        let found = lines("[![badge](b.svg)](https://x) [Docs][d] [plain]");
        assert_eq!(found, [text("Docs [plain]")]);
    }

    #[test]
    fn emphasis_goes_and_names_with_underscores_stay() {
        let found = lines("use `my_mod_id` and __bold__ *soft* ~~gone~~\n> quoted");
        assert_eq!(found, [text("use my_mod_id and bold soft gone quoted")]);
    }

    #[test]
    fn code_and_tables_are_shown_line_by_line() {
        let source = "```toml\nkey = \"H\"\n```\n| Key | Does |\n|---|---|\n| H | Opens |";
        let found = lines(source);
        let wanted = [
            text("key = \"H\""),
            text("Key \u{b7} Does"),
            text("H \u{b7} Opens"),
        ];
        assert_eq!(found, wanted);
        assert!(lines("").is_empty());
        assert!(lines("\n\n  \n").is_empty());
    }
}

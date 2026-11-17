//! A small scanner for the pages of the search engines.
//!
//! The crate has no HTML parser, and the search engines serve pages that are
//! simple enough to read with a scanner: the results sit in a block with a
//! class, the title is an anchor, and the snippet is a sibling. This module
//! finds those blocks, and it does it without pulling in a dependency.

/// One element of a page.
#[derive(Debug, Clone)]
pub struct Element {
    /// The name of the tag, in lower case.
    pub tag: String,
    /// The attributes of the tag, with the names in lower case.
    pub attributes: Vec<(String, String)>,
    /// The markup inside the element.
    pub inner: String,
}

impl Element {
    /// The value of an attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Whether the class of the element holds a token.
    pub fn has_class(&self, token: &str) -> bool {
        self.attribute("class")
            .is_some_and(|classes| classes.split_whitespace().any(|class| class == token))
    }

    /// The text of the element, without the tags.
    pub fn text(&self) -> String {
        text_of(&self.inner)
    }
}

/// The elements of a page with a tag and a class token.
pub fn elements_with_class(html: &str, tag: &str, class: &str) -> Vec<Element> {
    // An empty token matches every element of that tag.
    elements_where(html, tag, |attributes| {
        class.is_empty()
            || attributes.iter().any(|(key, value)| {
                key == "class" && value.split_whitespace().any(|candidate| candidate == class)
            })
    })
}

/// The elements of a page with a tag and an attribute of an exact value.
///
/// The engines that mark their results with `data-test-id` name no class, so
/// the blocks are found by the value of that attribute instead.
pub fn elements_with_attribute(html: &str, tag: &str, name: &str, value: &str) -> Vec<Element> {
    elements_where(html, tag, |attributes| {
        attributes
            .iter()
            .any(|(key, attribute)| key == name && attribute == value)
    })
}

/// The elements of a page with a tag whose attributes pass a test.
fn elements_where<F>(html: &str, tag: &str, matches: F) -> Vec<Element>
where
    F: Fn(&[(String, String)]) -> bool,
{
    let mut out: Vec<Element> = Vec::new();
    let tag = tag.to_ascii_lowercase();
    let mut cursor = 0;

    while let Some(at) = find_tag(html, &tag, cursor) {
        cursor = at + 1;
        let Some(open_end) = html[at..].find('>').map(|end| at + end + 1) else {
            break;
        };
        let opening = &html[at..open_end];
        // A tag that closes itself has no content.
        if opening.trim_end().ends_with('/') {
            continue;
        }
        let Some((attributes, self_closing)) = parse_attributes(opening) else {
            continue;
        };
        if self_closing || !matches(&attributes) {
            continue;
        }
        let Some(inner_end) = matching_close(html, &tag, open_end) else {
            continue;
        };
        out.push(Element {
            tag: tag.clone(),
            attributes,
            inner: html[open_end..inner_end].to_string(),
        });
        cursor = inner_end;
    }
    out
}

/// The first element of a page with a tag and a class token.
pub fn first_element_with_class(html: &str, tag: &str, class: &str) -> Option<Element> {
    elements_with_class(html, tag, class).into_iter().next()
}

/// The text of a piece of markup, without the tags and with the entities
/// decoded.
pub fn text_of(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut inside = false;
    for character in markup.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(character),
            _ => {}
        }
    }
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Decode the entities of a text: the named ones the engines emit, and the
/// numeric ones.
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '&' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        // The entity ends at the semicolon.
        let mut end = index + 1;
        while end < bytes.len() && end - index <= 12 && bytes[end] != ';' {
            end += 1;
        }
        if end >= bytes.len() || bytes[end] != ';' {
            out.push('&');
            index += 1;
            continue;
        }
        let name: String = bytes[index + 1..end].iter().collect();
        let replacement = match name.as_str() {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "hellip" => Some('…'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "rsquo" => Some('\''),
            "lsquo" => Some('\''),
            "ldquo" => Some('"'),
            "rdquo" => Some('"'),
            _ => {
                if let Some(digits) = name.strip_prefix('#') {
                    let code =
                        if let Some(hex) = digits.strip_prefix('x').or(digits.strip_prefix('X')) {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            digits.parse::<u32>().ok()
                        };
                    code.and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match replacement {
            Some(character) => {
                out.push(character);
                index = end + 1;
            }
            None => {
                out.push('&');
                index += 1;
            }
        }
    }
    out
}

/// The attributes of an opening tag.
fn parse_attributes(opening: &str) -> Option<(Vec<(String, String)>, bool)> {
    let inner = opening
        .strip_prefix('<')?
        .strip_suffix('>')
        .unwrap_or(opening)
        .trim();
    let self_closing = inner.ends_with('/');
    let inner = inner.trim_end_matches('/').trim();
    let mut attributes: Vec<(String, String)> = Vec::new();
    let characters: Vec<char> = inner.chars().collect();
    let mut index = 0;

    // The name of the tag.
    while index < characters.len() && !characters[index].is_whitespace() {
        index += 1;
    }
    while index < characters.len() {
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        if index >= characters.len() {
            break;
        }
        let start = index;
        while index < characters.len()
            && !characters[index].is_whitespace()
            && characters[index] != '='
            && characters[index] != '/'
        {
            index += 1;
        }
        let name: String = characters[start..index]
            .iter()
            .collect::<String>()
            .to_lowercase();
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        if index >= characters.len() || characters[index] != '=' {
            if !name.is_empty() {
                attributes.push((name, String::new()));
            }
            continue;
        }
        index += 1;
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        if index >= characters.len() {
            break;
        }
        let quote = characters[index];
        let value = if quote == '"' || quote == '\'' {
            index += 1;
            let start = index;
            while index < characters.len() && characters[index] != quote {
                index += 1;
            }
            let value: String = characters[start..index].iter().collect();
            index += 1;
            value
        } else {
            let start = index;
            while index < characters.len()
                && !characters[index].is_whitespace()
                && characters[index] != '>'
            {
                index += 1;
            }
            characters[start..index].iter().collect()
        };
        if !name.is_empty() {
            attributes.push((name, decode_entities(&value)));
        }
    }
    Some((attributes, self_closing))
}

/// The offset of the next opening tag with a name.
fn find_tag(html: &str, tag: &str, from: usize) -> Option<usize> {
    let needle = format!("<{tag}");
    let mut cursor = from;
    while let Some(at) = html[cursor..].find(&needle) {
        let at = cursor + at;
        // The name has to end here: `<div` must not match `<division`.
        let after = html[at + needle.len()..].chars().next();
        if after.is_some_and(|c| c.is_whitespace() || c == '>' || c == '/') {
            return Some(at);
        }
        cursor = at + needle.len();
    }
    None
}

/// The offset of the closing tag that closes the tag that was opened.
fn matching_close(html: &str, tag: &str, from: usize) -> Option<usize> {
    let mut depth = 1;
    let mut cursor = from;
    while cursor < html.len() {
        let open = find_tag(html, tag, cursor);
        let close = html[cursor..]
            .find(&format!("</{tag}"))
            .map(|at| cursor + at);
        match (open, close) {
            (None, None) => return None,
            (Some(open), Some(close)) if close < open => {
                depth -= 1;
                if depth == 0 {
                    let end = html[close..].find('>').map(|end| close + end + 1)?;
                    return Some(end);
                }
                cursor = close + tag.len() + 2;
            }
            (Some(open), _) => {
                depth += 1;
                cursor = open + tag.len() + 1;
            }
            (_, Some(close)) => {
                depth -= 1;
                if depth == 0 {
                    let end = html[close..].find('>').map(|end| close + end + 1)?;
                    return Some(end);
                }
                cursor = close + tag.len() + 2;
            }
        }
    }
    None
}

/// The value of an attribute of a tag.
pub fn attribute(tag: &str, name: &str) -> Option<String> {
    let (attributes, _) = parse_attributes(tag)?;
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

/// The hidden inputs of a form, by their name.
pub fn hidden_inputs(form: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cursor = 0;
    while let Some(at) = find_tag(form, "input", cursor) {
        cursor = at + 1;
        let Some(open_end) = form[at..].find('>').map(|end| at + end + 1) else {
            break;
        };
        let opening = &form[at..open_end];
        let Some((attributes, _)) = parse_attributes(opening) else {
            continue;
        };
        let name = attributes
            .iter()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.clone());
        let value = attributes
            .iter()
            .find(|(key, _)| key == "value")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        if let Some(name) = name {
            out.push((name, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
    <div class="result web-result">
      <a class="result__a" href="https://rust-lang.org/"><b>Rust</b> Programming Language</a>
      <a class="result__snippet" href="https://rust-lang.org/">Rust is a fast language.</a>
    </div>
    <div class="result">
      <a class="result__a" href="https://doc.rust-lang.org/">The Rust book</a>
    </div>
"#;

    #[test]
    fn the_blocks_are_found() {
        let blocks = elements_with_class(PAGE, "div", "result");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].inner.contains("result__a"));
    }

    #[test]
    fn the_blocks_marked_with_an_attribute_are_found() {
        let page = r#"
        <article data-test-id="organic-result"><h2 data-test-id="result-title">Rust</h2></article>
        <article data-test-id="news-result"><h2 data-test-id="result-title">News</h2></article>
        "#;
        let blocks = elements_with_attribute(page, "article", "data-test-id", "organic-result");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].inner.contains("Rust"));
    }

    #[test]
    fn the_attributes_are_read() {
        let blocks = elements_with_class(PAGE, "div", "result");
        let anchors = elements_with_class(&blocks[0].inner, "a", "result__a");
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].attribute("href"), Some("https://rust-lang.org/"));
        assert_eq!(anchors[0].text(), "Rust Programming Language");
    }

    #[test]
    fn the_text_of_a_markup_is_cleaned() {
        assert_eq!(text_of("<b>Hello</b> &amp; goodbye"), "Hello & goodbye");
        assert_eq!(text_of("a &nbsp; b"), "a b");
        assert_eq!(text_of("&#65;&#x42;"), "AB");
    }

    #[test]
    fn the_hidden_inputs_of_a_form_are_read() {
        let form = r#"<form action="/sp/search"><input type="hidden" name="sc" value="token"><input type="hidden" name="cat" value="web"></form>"#;
        let inputs = hidden_inputs(form);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0], ("sc".to_string(), "token".to_string()));
        assert_eq!(inputs[1], ("cat".to_string(), "web".to_string()));
    }
}

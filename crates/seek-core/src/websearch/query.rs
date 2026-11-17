//! The structured query of a web search (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner): `web/search/query.ts`.
//!
//! Agents habitually embed Google-style directives in a search query — `site:`,
//! `before:`/`after:`, `inurl:`, `filetype:`, quoted phrases, `OR` groups,
//! `-exclusions` — whether the engine behind the search parses them or not. This
//! module turns a raw query into a [`StructuredQuery`] so every provider can:
//!
//! 1. map the constraints onto the parameters its own API has (Perplexity
//!    `search_domain_filter`, Tavily `include_domains`, Exa date bounds, …),
//! 2. rebuild a query that holds only the syntax the engine understands
//!    ([`format_query`]), and
//! 3. post-filter the returned sources leniently ([`apply_query_constraints`]): a
//!    constraint that would leave nothing is dropped and reported, rather than
//!    returning nothing at all.

use super::types::SearchSource;

/// One free-text token of the query: everything that is not a directive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueryTerm {
    /// The term, without the quotes and without the operator prefixes.
    pub text: String,
    /// A quoted exact phrase (`"like this"`) or a verbatim one (`+term`).
    pub phrase: bool,
    /// Excluded, with `-term` or `NOT term`.
    pub negated: bool,
    /// The group of the term: the terms of a group are alternatives
    /// (`a OR b`); the terms without a group are AND-ed.
    pub group: Option<u32>,
}

/// A raw query, split into free text and every constraint it carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructuredQuery {
    /// The query as it was written.
    pub raw: String,
    /// The free text, with the directives taken out. The phrases stay quoted,
    /// the exclusions keep the `-`, the groups keep the `OR`.
    pub text: String,
    /// The free-text terms, in the order they were written.
    pub terms: Vec<QueryTerm>,
    /// The `site:`/`domain:`/`host:` includes: any of them matches. Lower case,
    /// without the scheme, and it may carry a path (`github.com/anthropics`).
    pub sites: Vec<String>,
    /// The `-site:` exclusions.
    pub excluded_sites: Vec<String>,
    /// The `inurl:`/`url:`/`allinurl:` substrings: all of them must be in the URL.
    pub in_url: Vec<String>,
    /// The `-inurl:` substrings: none of them may be in the URL.
    pub excluded_in_url: Vec<String>,
    /// The `intitle:`/`title:`/`allintitle:` substrings: all of them must be in
    /// the title.
    pub in_title: Vec<String>,
    /// The `-intitle:` substrings.
    pub excluded_in_title: Vec<String>,
    /// The `intext:`/`inbody:`/`inanchor:`/`allintext:` substrings. The snippets
    /// are partial, so these are not post-filterable: they only shape the query.
    pub in_text: Vec<String>,
    /// The `-intext:` substrings.
    pub excluded_in_text: Vec<String>,
    /// The `filetype:`/`ext:` extensions: any of them matches. Lower case, no
    /// leading dot.
    pub filetypes: Vec<String>,
    /// The `-filetype:`/`-ext:` extensions.
    pub excluded_filetypes: Vec<String>,
    /// The lower bound of the publication date, from `after:`/`since:`: an ISO
    /// `YYYY-MM-DD`.
    pub after: Option<String>,
    /// The upper bound of the publication date, from `before:`/`until:`: an ISO
    /// `YYYY-MM-DD`. The bound is exclusive.
    pub before: Option<String>,
    /// The language, from `lang:`/`language:`: lower case (`en`, `en-us`).
    pub lang: Option<String>,
    /// Whether a directive or a boolean operator was recognized.
    pub has_directives: bool,
    /// Whether a constraint that can be enforced on the results is set.
    pub has_constraints: bool,
}

/// What a target engine understands of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuerySyntax {
    /// `"quoted phrases"`.
    pub phrases: bool,
    /// `-term` exclusions. Without it, the negated terms are dropped.
    pub negation: bool,
    /// `OR` between the alternatives. Without it, the groups are flattened.
    pub or: bool,
    /// `site:`/`-site:`.
    pub site: bool,
    /// `inurl:`/`-inurl:`.
    pub in_url: bool,
    /// `intitle:`/`-intitle:`.
    pub in_title: bool,
    /// `intext:`/`-intext:`.
    pub in_text: bool,
    /// `filetype:`/`-filetype:`.
    pub filetype: bool,
    /// `before:`/`after:` with ISO dates.
    pub date_range: bool,
}

/// The whole Google-style syntax: the engines that parse the classic operator
/// set (Google, Startpage, Ecosia, Brave, Kagi, Mojeek, SearXNG…).
pub const GOOGLE_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
    phrases: true,
    negation: true,
    or: true,
    site: true,
    in_url: true,
    in_title: true,
    in_text: true,
    filetype: true,
    date_range: true,
};

/// The field a directive belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Site,
    InUrl,
    InTitle,
    InText,
    Filetype,
    Before,
    After,
    Lang,
}

/// The field of a directive name.
fn field_of(name: &str) -> Option<Field> {
    Some(match name {
        "site" | "domain" | "host" => Field::Site,
        "inurl" | "url" => Field::InUrl,
        "intitle" | "title" => Field::InTitle,
        "intext" | "inbody" | "inanchor" => Field::InText,
        "filetype" | "ext" => Field::Filetype,
        "before" | "until" => Field::Before,
        "after" | "since" => Field::After,
        "lang" | "language" => Field::Lang,
        _ => return None,
    })
}

/// The `allin*:` directives, and the field every following term goes to.
fn all_mode_of(name: &str) -> Option<Field> {
    Some(match name {
        "allintitle" => Field::InTitle,
        "allinurl" => Field::InUrl,
        "allintext" => Field::InText,
        _ => return None,
    })
}

/// One token of the raw query.
#[derive(Debug, Clone)]
struct RawToken {
    text: String,
    /// The whole token was a quoted phrase.
    quoted: bool,
    /// The value of a directive was quoted (`intitle:"a b"`).
    quoted_value: bool,
}

/// Whether the character opens or closes a quotation.
fn is_quote(character: char) -> bool {
    character == '"' || character == '\u{201c}' || character == '\u{201d}'
}

/// Split the query into tokens, honouring the quoted spans and the parens.
fn tokenize(raw: &str) -> Vec<RawToken> {
    let mut tokens: Vec<RawToken> = Vec::new();
    let characters: Vec<char> = raw.chars().collect();
    let n = characters.len();
    let mut index = 0;

    while index < n {
        let character = characters[index];
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        if is_quote(character) {
            let mut end = index + 1;
            let mut buffer = String::new();
            while end < n && !is_quote(characters[end]) {
                buffer.push(characters[end]);
                end += 1;
            }
            let text = buffer.trim().to_string();
            if !text.is_empty() {
                tokens.push(RawToken {
                    text,
                    quoted: true,
                    quoted_value: false,
                });
            }
            index = end + 1;
            continue;
        }

        // A bare word. A quote right after a `name:` swallows the quoted span
        // into the same token (`intitle:"budget tips"`).
        let mut buffer = String::new();
        let mut quoted_value = false;
        while index < n && !characters[index].is_whitespace() {
            let current = characters[index];
            if is_quote(current) && buffer.ends_with(':') {
                let mut end = index + 1;
                while end < n && !is_quote(characters[end]) {
                    buffer.push(characters[end]);
                    end += 1;
                }
                quoted_value = true;
                index = end + 1;
                continue;
            }
            if is_quote(current) {
                // `foo"bar`: the word stops here and the quote starts a phrase.
                break;
            }
            buffer.push(current);
            index += 1;
        }
        if !buffer.is_empty() {
            tokens.push(RawToken {
                text: buffer,
                quoted: false,
                quoted_value,
            });
        }
    }
    split_parens(tokens)
}

/// Split the leading `(` and the unbalanced trailing `)` into tokens of their
/// own, so `(react OR vue)` parses and `site:wikipedia.org/Foo_(bar)` stays whole.
fn split_parens(tokens: Vec<RawToken>) -> Vec<RawToken> {
    let mut out: Vec<RawToken> = Vec::new();
    for token in tokens {
        if token.quoted || token.quoted_value {
            out.push(token);
            continue;
        }
        let mut text = token.text;
        while text.starts_with('(') {
            out.push(RawToken {
                text: "(".into(),
                quoted: false,
                quoted_value: false,
            });
            text = text[1..].to_string();
        }
        let mut trailing = 0;
        while text.ends_with(')') {
            let body = &text[..text.len() - 1];
            let mut depth = 0i32;
            for character in body.chars() {
                if character == '(' {
                    depth += 1;
                } else if character == ')' {
                    depth -= 1;
                }
            }
            if depth > 0 {
                break;
            }
            text = body.to_string();
            trailing += 1;
        }
        if !text.is_empty() {
            out.push(RawToken {
                text,
                quoted: false,
                quoted_value: false,
            });
        }
        for _ in 0..trailing {
            out.push(RawToken {
                text: ")".into(),
                quoted: false,
                quoted_value: false,
            });
        }
    }
    out
}

/// The parts of a `name:value` token: the sign, the name and the value.
fn directive_parts(token: &str) -> Option<(bool, String, String)> {
    let (negated, rest) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token.strip_prefix('+').unwrap_or(token)),
    };
    let colon = rest.find(':')?;
    let name = &rest[..colon];
    let value = &rest[colon + 1..];
    // The name is a letter, then letters and dashes.
    let mut characters = name.chars();
    let first = characters.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if !characters.all(|c| c.is_ascii_alphabetic() || c == '-') {
        return None;
    }
    Some((negated, name.to_ascii_lowercase(), value.to_string()))
}

/// Whether a token is an operator or a directive: a bare `name:` must not
/// adopt one of them as its value.
fn is_reserved(token: &str) -> bool {
    if matches!(
        token,
        "(" | ")" | "OR" | "AND" | "NOT" | "|" | "||" | "&&" | "!"
    ) {
        return true;
    }
    let Some((_, name, _)) = directive_parts(token) else {
        return false;
    };
    field_of(&name).is_some() || all_mode_of(&name).is_some()
}

/// An ISO date out of the parts of a date.
fn iso_date(year: i64, month: i64, day: i64) -> Option<String> {
    if !(1000..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Parse the value of a `before:`/`after:` into an ISO `YYYY-MM-DD`.
///
/// It takes `YYYY`, `YYYY-MM` and `YYYY-MM-DD` (with `-`, `/` or `.` as the
/// separator) and `MM/DD/YYYY` (the day comes first when the first field is
/// above 12). A bare year or month resolves to the first day of the period,
/// like Google's `after:2024` ≙ `after:2024-01-01`.
pub fn parse_date_value(value: &str) -> Option<String> {
    let text = value.trim();
    let parts: Vec<&str> = text.split(['-', '/', '.']).collect();

    // The year first: `YYYY`, `YYYY-MM`, `YYYY-MM-DD`.
    if parts.len() <= 3
        && let Some(year) = parts[0].parse::<i64>().ok().filter(|_| parts[0].len() == 4)
    {
        let month = match parts.get(1) {
            Some(part) => part.parse::<i64>().ok()?,
            None => 1,
        };
        let day = match parts.get(2) {
            Some(part) => part.parse::<i64>().ok()?,
            None => 1,
        };
        if parts.len() > 1 && parts[1].len() > 2 {
            return None;
        }
        if parts.len() > 2 && parts[2].len() > 2 {
            return None;
        }
        return iso_date(year, month, day);
    }

    // The month first: `MM/DD/YYYY`.
    if parts.len() == 3 {
        let first = parts[0].parse::<i64>().ok()?;
        let second = parts[1].parse::<i64>().ok()?;
        let year = parts[2].parse::<i64>().ok()?;
        if parts[2].len() != 4 {
            return None;
        }
        let (month, day) = if first > 12 && second <= 12 {
            (second, first)
        } else {
            (first, second)
        };
        return iso_date(year, month, day);
    }
    None
}

/// Lower case a `site:` value, and take the scheme, the `*.` and the trailing
/// slash and dot off it.
fn normalize_site(value: &str) -> String {
    let mut site = value.trim().to_lowercase();
    // The scheme: `https://`, `ftp://`, …
    if let Some(rest) = site.split_once("://") {
        let scheme = rest.0;
        if !scheme.is_empty()
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-')
        {
            site = rest.1.to_string();
        }
    }
    if let Some(rest) = site.strip_prefix("*.") {
        site = rest.to_string();
    }
    site = site.trim_end_matches(['/', '.']).to_string();
    site
}

/// Parse a raw query into a [`StructuredQuery`].
///
/// It is lenient by construction: an unknown `name:value` token (a URL, a
/// `C:\path`, `TS2345:`, some jargon) stays in the free text, and a directive
/// with a value that cannot be read (`before:someday`) degrades to a plain term
/// instead of being dropped.
pub fn parse_search_query(raw: &str) -> StructuredQuery {
    let mut query = StructuredQuery {
        raw: raw.to_string(),
        ..Default::default()
    };

    let tokens = tokenize(raw);
    let mut negate_next = false;
    let mut or_pending = false;
    let mut last_was_term = false;
    let mut group_sequence = 0u32;
    let mut all_mode: Option<Field> = None;

    for index in 0..tokens.len() {
        let token = &tokens[index];

        if token.quoted {
            push_term(
                &mut query,
                &mut negate_next,
                &mut or_pending,
                &mut last_was_term,
                &mut group_sequence,
                all_mode,
                &token.text,
                true,
            );
            continue;
        }

        // The boolean operators and the grouping parens.
        if token.text == "(" || token.text == ")" {
            continue;
        }
        if token.text == "OR" || token.text == "|" || token.text == "||" {
            or_pending = true;
            query.has_directives = true;
            continue;
        }
        if token.text == "AND" || token.text == "&&" {
            query.has_directives = true;
            continue;
        }
        if token.text == "NOT" || token.text == "!" {
            negate_next = true;
            query.has_directives = true;
            continue;
        }
        if token.text == "-" || token.text == "+" {
            // The tokenizer splits `-"exact phrase"` into `-` and the phrase:
            // the negation carries over.
            if token.text == "-" && tokens.get(index + 1).is_some_and(|next| next.quoted) {
                negate_next = true;
            }
            continue;
        }

        let parts = directive_parts(&token.text);
        let name = parts.as_ref().map(|(_, name, _)| name.clone());
        let all_field = name.as_deref().and_then(all_mode_of);
        let field = name.as_deref().and_then(field_of);

        if let (Some(parts), Some(field)) = (&parts, all_field) {
            all_mode = Some(field);
            query.has_directives = true;
            // `allintitle:budget tips`: the value of the directive and every
            // term that follows it.
            let inline = parts.2.trim().to_string();
            if !inline.is_empty() {
                push_constraint(&mut query, field, &inline, parts.0);
            }
            or_pending = false;
            last_was_term = false;
            continue;
        }

        if let (Some(parts), Some(field)) = (&parts, field) {
            let mut value = parts.2.trim().to_string();
            // `site: example.com`: the next plain token becomes the value.
            if value.is_empty()
                && let Some(next) = tokens.get(index + 1)
                && (next.quoted || !is_reserved(&next.text))
            {
                value = next.text.trim().to_string();
            }
            if value.is_empty() {
                query.has_directives = true;
                continue;
            }
            let negated = parts.0 || negate_next;
            negate_next = false;
            match field {
                Field::Before | Field::After => {
                    let Some(iso) = parse_date_value(&value) else {
                        push_term(
                            &mut query,
                            &mut negate_next,
                            &mut or_pending,
                            &mut last_was_term,
                            &mut group_sequence,
                            all_mode,
                            &token.text,
                            false,
                        );
                        continue;
                    };
                    if field == Field::Before {
                        query.before = Some(iso);
                    } else {
                        query.after = Some(iso);
                    }
                    query.has_directives = true;
                    or_pending = false;
                    last_was_term = false;
                }
                Field::Lang => {
                    query.lang = Some(value.to_lowercase());
                    query.has_directives = true;
                    or_pending = false;
                    last_was_term = false;
                }
                _ => push_constraint(&mut query, field, &value, negated),
            }
            continue;
        }

        // A plain term, with an optional `-` or `+` in front of it.
        let mut text = token.text.clone();
        if text.starts_with('-') && text.len() > 1 {
            negate_next = true;
            query.has_directives = true;
            text = text.trim_start_matches('-').to_string();
            if text.is_empty() {
                continue;
            }
            // `-site:x` arrives split only when it is written `- site:x`: the
            // directive is checked again.
            if let Some((_, name, value)) = directive_parts(&text)
                && let Some(field) = field_of(&name)
                && !matches!(field, Field::Before | Field::After | Field::Lang)
            {
                negate_next = false;
                push_constraint(&mut query, field, value.trim(), true);
                continue;
            }
            push_term(
                &mut query,
                &mut negate_next,
                &mut or_pending,
                &mut last_was_term,
                &mut group_sequence,
                all_mode,
                &text,
                false,
            );
            continue;
        }
        if text.starts_with('+') && text.len() > 1 {
            // The old Google `+term`: a verbatim word, so an exact phrase.
            push_term(
                &mut query,
                &mut negate_next,
                &mut or_pending,
                &mut last_was_term,
                &mut group_sequence,
                all_mode,
                &text[1..],
                true,
            );
            query.has_directives = true;
            continue;
        }
        push_term(
            &mut query,
            &mut negate_next,
            &mut or_pending,
            &mut last_was_term,
            &mut group_sequence,
            all_mode,
            &text,
            false,
        );
    }

    query.text = render_terms(
        &query.terms,
        &QuerySyntax {
            phrases: true,
            negation: true,
            or: true,
            ..Default::default()
        },
    );
    query.has_constraints = !query.sites.is_empty()
        || !query.excluded_sites.is_empty()
        || !query.in_url.is_empty()
        || !query.excluded_in_url.is_empty()
        || !query.in_title.is_empty()
        || !query.excluded_in_title.is_empty()
        || !query.filetypes.is_empty()
        || !query.excluded_filetypes.is_empty()
        || query.before.is_some()
        || query.after.is_some();
    query
}

/// Put a constraint of the query in its place.
fn push_constraint(query: &mut StructuredQuery, field: Field, value: &str, negated: bool) {
    query.has_directives = true;
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match field {
        Field::Site => {
            let site = normalize_site(value);
            if !site.is_empty() {
                if negated {
                    query.excluded_sites.push(site);
                } else {
                    query.sites.push(site);
                }
            }
        }
        Field::InUrl => {
            if negated {
                query.excluded_in_url.push(value.to_string());
            } else {
                query.in_url.push(value.to_string());
            }
        }
        Field::InTitle => {
            if negated {
                query.excluded_in_title.push(value.to_string());
            } else {
                query.in_title.push(value.to_string());
            }
        }
        Field::InText => {
            if negated {
                query.excluded_in_text.push(value.to_string());
            } else {
                query.in_text.push(value.to_string());
            }
        }
        Field::Filetype => {
            let extension = value.to_lowercase().trim_start_matches('.').to_string();
            if !extension.is_empty() {
                if negated {
                    query.excluded_filetypes.push(extension);
                } else {
                    query.filetypes.push(extension);
                }
            }
        }
        _ => {}
    }
}

/// Put a free-text term in its place.
#[allow(clippy::too_many_arguments)]
fn push_term(
    query: &mut StructuredQuery,
    negate_next: &mut bool,
    or_pending: &mut bool,
    last_was_term: &mut bool,
    group_sequence: &mut u32,
    all_mode: Option<Field>,
    text: &str,
    phrase: bool,
) {
    let negated = *negate_next;
    *negate_next = false;
    if let Some(field) = all_mode {
        push_constraint(query, field, text, negated);
        return;
    }
    let mut term = QueryTerm {
        text: text.to_string(),
        phrase,
        negated,
        group: None,
    };
    if *or_pending
        && *last_was_term
        && let Some(previous) = query.terms.last_mut()
    {
        if previous.group.is_none() {
            *group_sequence += 1;
            previous.group = Some(*group_sequence);
        }
        term.group = previous.group;
    }
    *or_pending = false;
    *last_was_term = true;
    query.terms.push(term);
}

/// Quote a directive value that holds a space.
fn quote_value(value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// Render the free-text terms in the syntax of the target engine.
fn render_terms(terms: &[QueryTerm], syntax: &QuerySyntax) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut index = 0;
    while index < terms.len() {
        let term = &terms[index];
        if let Some(group) = term.group
            && syntax.or
        {
            let mut members: Vec<String> = Vec::new();
            let mut cursor = index;
            while cursor < terms.len() && terms[cursor].group == Some(group) {
                if let Some(rendered) = render_term(&terms[cursor], syntax) {
                    members.push(rendered);
                }
                cursor += 1;
            }
            index = cursor;
            if members.len() > 1 {
                parts.push(format!("({})", members.join(" OR ")));
            } else if let Some(member) = members.first() {
                parts.push(member.clone());
            }
            continue;
        }
        if let Some(rendered) = render_term(term, syntax) {
            parts.push(rendered);
        }
        index += 1;
    }
    parts.join(" ")
}

/// Render one term in the syntax of the target engine.
fn render_term(term: &QueryTerm, syntax: &QuerySyntax) -> Option<String> {
    if term.negated && !syntax.negation {
        return None;
    }
    let body = if term.phrase && syntax.phrases {
        format!("\"{}\"", term.text)
    } else {
        term.text.clone()
    };
    Some(if term.negated {
        format!("-{body}")
    } else {
        body
    })
}

/// Rebuild a query string for an engine with the given syntax.
///
/// The constraints the engine does not understand are left out: the caller
/// maps them onto the parameters of its API, or relies on
/// [`apply_query_constraints`]. The result is never empty for a non-empty
/// input: a query of directives only falls back to the values of the
/// constraints as keywords, and then to the raw query — an engine that
/// searches *something* beats a query that is empty.
pub fn format_query(query: &StructuredQuery, syntax: &QuerySyntax) -> String {
    let mut parts: Vec<String> = Vec::new();
    let text = render_terms(&query.terms, syntax);
    if !text.is_empty() {
        parts.push(text);
    }

    if syntax.site {
        if query.sites.len() > 1 && syntax.or {
            let group: Vec<String> = query
                .sites
                .iter()
                .map(|site| format!("site:{site}"))
                .collect();
            parts.push(format!("({})", group.join(" OR ")));
        } else {
            for site in &query.sites {
                parts.push(format!("site:{site}"));
            }
        }
        for site in &query.excluded_sites {
            parts.push(format!("-site:{site}"));
        }
    }
    if syntax.in_url {
        for value in &query.in_url {
            parts.push(format!("inurl:{}", quote_value(value)));
        }
        for value in &query.excluded_in_url {
            parts.push(format!("-inurl:{}", quote_value(value)));
        }
    }
    if syntax.in_title {
        for value in &query.in_title {
            parts.push(format!("intitle:{}", quote_value(value)));
        }
        for value in &query.excluded_in_title {
            parts.push(format!("-intitle:{}", quote_value(value)));
        }
    }
    if syntax.in_text {
        for value in &query.in_text {
            parts.push(format!("intext:{}", quote_value(value)));
        }
        for value in &query.excluded_in_text {
            parts.push(format!("-intext:{}", quote_value(value)));
        }
    }
    if syntax.filetype {
        if query.filetypes.len() > 1 && syntax.or {
            let group: Vec<String> = query
                .filetypes
                .iter()
                .map(|extension| format!("filetype:{extension}"))
                .collect();
            parts.push(format!("({})", group.join(" OR ")));
        } else {
            for extension in &query.filetypes {
                parts.push(format!("filetype:{extension}"));
            }
        }
        for extension in &query.excluded_filetypes {
            parts.push(format!("-filetype:{extension}"));
        }
    }
    if syntax.date_range {
        if let Some(after) = &query.after {
            parts.push(format!("after:{after}"));
        }
        if let Some(before) = &query.before {
            parts.push(format!("before:{before}"));
        }
    }

    let mut result = parts.join(" ").trim().to_string();
    if result.is_empty() {
        // A query of directives only, and the engine understands no directive:
        // the values of the constraints become plain keywords.
        let mut fallback: Vec<&str> = Vec::new();
        fallback.extend(query.sites.iter().map(String::as_str));
        fallback.extend(query.in_title.iter().map(String::as_str));
        fallback.extend(query.in_url.iter().map(String::as_str));
        fallback.extend(query.in_text.iter().map(String::as_str));
        fallback.extend(query.filetypes.iter().map(String::as_str));
        result = fallback.join(" ").trim().to_string();
    }
    if result.is_empty() {
        result = query.raw.trim().to_string();
    }
    result
}

/// Build the query for an engine that scrapes a page and needs no key (Google,
/// Startpage, DuckDuckGo, Ecosia, Mojeek, SearXNG, and the Public Web
/// fan-out over them).
///
/// The directives are canonicalized with [`format_query`], after the operators
/// that match nothing across the scrapers are demoted: the engines only match
/// `site:` against a bare domain (a path gives nothing everywhere), and
/// DuckDuckGo ignores `inurl:` altogether — so either operator empties the
/// result set without a word. The raw URL as a plain term matches fine, so the
/// bare domains stay and the paths and the `inurl:` values become plain
/// keywords. The demotion happens before the formatting, so the OR groups and
/// the quoted directives are covered too. The negated forms (`-site:`,
/// `-inurl:`) pass through untouched: demoting them would turn an exclusion
/// into a search term. The pipeline post-filter
/// ([`apply_query_constraints`]) enforces every demoted constraint on the
/// sources that come back. A query without directives passes through
/// byte-identical.
pub fn format_scraper_query(
    query: &str,
    parsed: Option<&StructuredQuery>,
    syntax: &QuerySyntax,
) -> String {
    let owned;
    let parsed = match parsed {
        Some(parsed) => parsed,
        None => {
            owned = parse_search_query(query);
            &owned
        }
    };
    if !parsed.has_directives {
        return query.to_string();
    }
    let demoted: Vec<String> = parsed
        .sites
        .iter()
        .filter(|site| site.contains('/'))
        .cloned()
        .chain(parsed.in_url.iter().cloned())
        .collect();
    let mut downgraded = parsed.clone();
    downgraded.sites.retain(|site| !site.contains('/'));
    downgraded.in_url.clear();
    for text in demoted {
        downgraded.terms.push(QueryTerm {
            text,
            ..Default::default()
        });
    }
    format_query(&downgraded, syntax)
}

/// The host and the path of a URL.
fn host_and_path(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let rest = rest.split(['?', '#']).next()?;
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    };
    if host.is_empty() {
        return None;
    }
    Some((host.to_lowercase(), path.to_string()))
}

/// Whether a URL is on a site: the host is the site or a subdomain of it.
///
/// When the site carries a path (`github.com/anthropics`), the path of the URL
/// has to start with it.
pub fn matches_site(url: &str, site: &str) -> bool {
    let Some((host, path)) = host_and_path(url) else {
        return false;
    };
    let (site_host, site_path) = match site.find('/') {
        Some(at) => (&site[..at], &site[at..]),
        None => (site, ""),
    };
    if host != site_host && !host.ends_with(&format!(".{site_host}")) {
        return false;
    }
    if !site_path.is_empty() && !path.to_lowercase().starts_with(&site_path.to_lowercase()) {
        return false;
    }
    true
}

/// Whether the path of a URL ends with `.extension`.
pub fn matches_filetype(url: &str, extension: &str) -> bool {
    let Some((_, path)) = host_and_path(url) else {
        return false;
    };
    path.to_lowercase().ends_with(&format!(".{extension}"))
}

/// The dimensions of the constraints of a query, with their label.
struct ConstraintDimension {
    label: String,
    test: Box<dyn Fn(&SearchSource) -> bool>,
}

/// The dimensions of the constraints of a query.
fn constraint_dimensions(query: &StructuredQuery) -> Vec<ConstraintDimension> {
    let mut dimensions: Vec<ConstraintDimension> = Vec::new();

    if !query.sites.is_empty() {
        let sites = query.sites.clone();
        dimensions.push(ConstraintDimension {
            label: sites
                .iter()
                .map(|site| format!("site:{site}"))
                .collect::<Vec<String>>()
                .join(" OR "),
            test: Box::new(move |source| sites.iter().any(|site| matches_site(&source.url, site))),
        });
    }
    if !query.excluded_sites.is_empty() {
        let sites = query.excluded_sites.clone();
        dimensions.push(ConstraintDimension {
            label: sites
                .iter()
                .map(|site| format!("-site:{site}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| !sites.iter().any(|site| matches_site(&source.url, site))),
        });
    }
    if !query.in_url.is_empty() {
        let values = query.in_url.clone();
        dimensions.push(ConstraintDimension {
            label: values
                .iter()
                .map(|value| format!("inurl:{value}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| {
                let url = source.url.to_lowercase();
                values
                    .iter()
                    .all(|value| url.contains(&value.to_lowercase()))
            }),
        });
    }
    if !query.excluded_in_url.is_empty() {
        let values = query.excluded_in_url.clone();
        dimensions.push(ConstraintDimension {
            label: values
                .iter()
                .map(|value| format!("-inurl:{value}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| {
                let url = source.url.to_lowercase();
                !values
                    .iter()
                    .any(|value| url.contains(&value.to_lowercase()))
            }),
        });
    }
    if !query.in_title.is_empty() {
        let values = query.in_title.clone();
        dimensions.push(ConstraintDimension {
            label: values
                .iter()
                .map(|value| format!("intitle:{value}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| {
                let title = source.title.to_lowercase();
                values
                    .iter()
                    .all(|value| title.contains(&value.to_lowercase()))
            }),
        });
    }
    if !query.excluded_in_title.is_empty() {
        let values = query.excluded_in_title.clone();
        dimensions.push(ConstraintDimension {
            label: values
                .iter()
                .map(|value| format!("-intitle:{value}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| {
                let title = source.title.to_lowercase();
                !values
                    .iter()
                    .any(|value| title.contains(&value.to_lowercase()))
            }),
        });
    }
    if !query.filetypes.is_empty() {
        let extensions = query.filetypes.clone();
        dimensions.push(ConstraintDimension {
            label: extensions
                .iter()
                .map(|extension| format!("filetype:{extension}"))
                .collect::<Vec<String>>()
                .join(" OR "),
            test: Box::new(move |source| {
                extensions
                    .iter()
                    .any(|extension| matches_filetype(&source.url, extension))
            }),
        });
    }
    if !query.excluded_filetypes.is_empty() {
        let extensions = query.excluded_filetypes.clone();
        dimensions.push(ConstraintDimension {
            label: extensions
                .iter()
                .map(|extension| format!("-filetype:{extension}"))
                .collect::<Vec<String>>()
                .join(" "),
            test: Box::new(move |source| {
                !extensions
                    .iter()
                    .any(|extension| matches_filetype(&source.url, extension))
            }),
        });
    }
    if query.after.is_some() || query.before.is_some() {
        let after = query.after.as_deref().and_then(parse_iso_seconds);
        let before = query.before.as_deref().and_then(parse_iso_seconds);
        let label = [
            query.after.as_deref().map(|after| format!("after:{after}")),
            query
                .before
                .as_deref()
                .map(|before| format!("before:{before}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<String>>()
        .join(" ");
        dimensions.push(ConstraintDimension {
            label,
            test: Box::new(move |source| {
                let Some(time) = source_time(source) else {
                    // A source without a date cannot break the bound.
                    return true;
                };
                if let Some(after) = after
                    && time < after
                {
                    return false;
                }
                if let Some(before) = before
                    && time >= before
                {
                    return false;
                }
                true
            }),
        });
    }
    dimensions
}

/// The result of [`apply_query_constraints`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintFilterResult {
    /// The sources that survived the filter: never empty when the input was not.
    pub sources: Vec<SearchSource>,
    /// The constraints that matched nothing and were relaxed instead of being
    /// enforced.
    pub dropped: Vec<String>,
}

/// The strict check of every constraint of a query against one source.
///
/// A source without a date passes the date bounds: a missing date is not proof
/// of a violation.
pub fn matches_query_constraints(source: &SearchSource, query: &StructuredQuery) -> bool {
    constraint_dimensions(query)
        .iter()
        .all(|dimension| (dimension.test)(source))
}

/// The lenient post-filter: every constraint of the query is applied in turn, and
/// the one that would leave nothing is skipped and reported.
///
/// The result is never empty for a non-empty input, so a directive that was
/// written wrong degrades to unfiltered results and a note, and not to a dead
/// search.
pub fn apply_query_constraints(
    sources: &[SearchSource],
    query: &StructuredQuery,
) -> ConstraintFilterResult {
    let mut current: Vec<SearchSource> = sources.to_vec();
    let mut dropped: Vec<String> = Vec::new();
    if current.is_empty() {
        return ConstraintFilterResult {
            sources: current,
            dropped,
        };
    }
    for dimension in constraint_dimensions(query) {
        let kept: Vec<SearchSource> = current
            .iter()
            .filter(|source| (dimension.test)(source))
            .cloned()
            .collect();
        if kept.is_empty() {
            dropped.push(dimension.label);
        } else {
            current = kept;
        }
    }
    ConstraintFilterResult {
        sources: current,
        dropped,
    }
}

/// The time of a source, in epoch seconds.
fn source_time(source: &SearchSource) -> Option<i64> {
    if let Some(age) = source.age_seconds {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        return Some(now - age as i64);
    }
    let published = source.published_date.as_deref()?;
    if let Some(seconds) = relative_age_seconds(published) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        return Some(now - seconds);
    }
    parse_iso_seconds(published)
}

/// The seconds of a date written as "3 days ago".
fn relative_age_seconds(text: &str) -> Option<i64> {
    let text = text.trim();
    let rest = text
        .strip_suffix(" ago")
        .or_else(|| text.strip_suffix(" ago"))?;
    let rest = rest.trim();
    let (number, unit) = rest.split_once(' ')?;
    let number: i64 = number.trim().parse().ok()?;
    let unit = unit.trim().to_lowercase();
    let seconds = match unit.as_str() {
        "m" | "min" | "minute" | "minutes" => 60,
        "h" | "hr" | "hour" | "hours" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 604_800,
        "mo" | "month" | "months" => 2_592_000,
        "y" | "yr" | "year" | "years" => 31_536_000,
        _ => return None,
    };
    if number > 0 {
        Some(number * seconds)
    } else {
        None
    }
}

/// The days since the epoch of a civil date, from Howard Hinnant's algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_of_year = (month + 9) % 12;
    let day_of_year = (153 * month_of_year + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The epoch seconds of an ISO date: `YYYY-MM-DD`, with an optional time.
pub fn parse_iso_seconds(text: &str) -> Option<i64> {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' {
        return None;
    }
    let month: i64 = text.get(5..7)?.parse().ok()?;
    if bytes[7] != b'-' {
        return None;
    }
    let day: i64 = text.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut seconds = days_from_civil(year, month, day) * 86_400;
    // The time, when there is one: `HH:MM`, and the seconds when there are some.
    if bytes.len() > 10 {
        let time = text.get(11..)?;
        let parts: Vec<&str> = time.split(':').collect();
        if let Some(hours) = parts.first().and_then(|part| part.parse::<i64>().ok()) {
            seconds += hours * 3_600;
        }
        if let Some(minutes) = parts.get(1).and_then(|part| part.parse::<i64>().ok()) {
            seconds += minutes * 60;
        }
        if let Some(whole) = parts.get(2) {
            let whole = whole.split('.').next().unwrap_or(whole);
            if let Ok(value) = whole.parse::<i64>() {
                seconds += value;
            }
        }
    }
    Some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> StructuredQuery {
        parse_search_query(raw)
    }

    #[test]
    fn a_plain_query_is_only_text() {
        let query = parse("rust programming language");
        assert_eq!(query.text, "rust programming language");
        assert!(!query.has_directives);
        assert!(!query.has_constraints);
        assert_eq!(query.terms.len(), 3);
    }

    #[test]
    fn a_site_directive_is_read() {
        let query = parse("rust site:github.com");
        assert_eq!(query.sites, vec!["github.com"]);
        assert!(query.has_directives);
        assert!(query.has_constraints);
        assert_eq!(query.text, "rust");
    }

    #[test]
    fn the_site_directive_takes_the_scheme_and_the_wildcard_off() {
        let query = parse("site:https://*.Example.com/");
        assert_eq!(query.sites, vec!["example.com"]);
    }

    #[test]
    fn a_site_may_carry_a_path() {
        let query = parse("site:github.com/anthropics claude");
        assert_eq!(query.sites, vec!["github.com/anthropics"]);
    }

    #[test]
    fn a_negated_site_is_an_exclusion() {
        let query = parse("rust -site:github.com");
        assert_eq!(query.excluded_sites, vec!["github.com"]);
        assert!(query.sites.is_empty());
    }

    #[test]
    fn the_directive_aliases_are_read() {
        let query = parse("domain:example.com inurl:login intitle:home intext:secret ext:pdf");
        assert_eq!(query.sites, vec!["example.com"]);
        assert_eq!(query.in_url, vec!["login"]);
        assert_eq!(query.in_title, vec!["home"]);
        assert_eq!(query.in_text, vec!["secret"]);
        assert_eq!(query.filetypes, vec!["pdf"]);
    }

    #[test]
    fn a_quoted_phrase_stays_a_phrase() {
        let query = parse("\"machine learning\" rust");
        assert!(query.terms[0].phrase);
        assert_eq!(query.terms[0].text, "machine learning");
        assert_eq!(query.text, "\"machine learning\" rust");
    }

    #[test]
    fn a_negated_term_is_negated() {
        let query = parse("rust -deprecated");
        assert!(query.terms[1].negated);
        assert_eq!(query.text, "rust -deprecated");
    }

    #[test]
    fn an_or_group_groups_its_terms() {
        let query = parse("rust OR golang");
        assert_eq!(query.terms[0].group, Some(1));
        assert_eq!(query.terms[1].group, Some(1));
        assert_eq!(query.text, "(rust OR golang)");
    }

    #[test]
    fn a_date_directive_is_read() {
        let query = parse("rust after:2024-01-15 before:2025-06-01");
        assert_eq!(query.after.as_deref(), Some("2024-01-15"));
        assert_eq!(query.before.as_deref(), Some("2025-06-01"));
    }

    #[test]
    fn a_bare_year_is_the_first_day_of_the_year() {
        assert_eq!(parse_date_value("2024").as_deref(), Some("2024-01-01"));
        assert_eq!(parse_date_value("2024/03").as_deref(), Some("2024-03-01"));
        assert_eq!(
            parse_date_value("2024.03.05").as_deref(),
            Some("2024-03-05")
        );
    }

    #[test]
    fn a_date_that_cannot_be_read_is_nothing() {
        assert!(parse_date_value("someday").is_none());
        assert!(parse_date_value("2024-13-01").is_none());
    }

    #[test]
    fn an_unknown_directive_stays_in_the_text() {
        let query = parse("TS2345: the type is wrong");
        assert_eq!(query.text, "TS2345: the type is wrong");
        assert!(!query.has_directives);
    }

    #[test]
    fn a_date_that_cannot_be_read_degrades_to_a_term() {
        let query = parse("rust before:someday");
        assert!(query.before.is_none());
        assert!(query.text.contains("before:someday"));
    }

    #[test]
    fn the_language_is_read() {
        let query = parse("rust language:EN-us");
        assert_eq!(query.lang.as_deref(), Some("en-us"));
    }

    #[test]
    fn an_all_directive_takes_the_terms_that_follow() {
        let query = parse("allintitle:budget tips for 2024");
        assert_eq!(query.in_title, vec!["budget", "tips", "for", "2024"]);
        assert!(query.terms.is_empty());
    }

    #[test]
    fn a_query_without_directives_passes_through() {
        let query = parse("rust programming");
        let formatted =
            format_scraper_query("rust programming", Some(&query), &GOOGLE_QUERY_SYNTAX);
        assert_eq!(formatted, "rust programming");
    }

    #[test]
    fn a_path_in_a_site_is_demoted_to_a_keyword() {
        // The engines match `site:` against a bare domain only.
        let query = parse("site:github.com/anthropics claude");
        let formatted = format_scraper_query("", Some(&query), &GOOGLE_QUERY_SYNTAX);
        assert!(!formatted.contains("site:"), "{formatted}");
        assert!(formatted.contains("github.com/anthropics"), "{formatted}");
        assert!(formatted.contains("claude"), "{formatted}");
    }

    #[test]
    fn the_syntax_of_the_engine_decides_what_is_emitted() {
        let query = parse("\"a phrase\" -bad site:x.com");
        let plain = format_query(
            &query,
            &QuerySyntax {
                phrases: true,
                negation: true,
                ..Default::default()
            },
        );
        assert!(plain.contains("\"a phrase\""), "{plain}");
        assert!(plain.contains("-bad"), "{plain}");
        assert!(!plain.contains("site:"), "{plain}");
    }

    #[test]
    fn a_query_of_directives_only_is_never_empty() {
        let query = parse("site:example.com");
        let formatted = format_query(&query, &QuerySyntax::default());
        assert_eq!(formatted, "example.com");
    }

    #[test]
    fn the_site_matcher_knows_the_subdomains() {
        assert!(matches_site("https://github.com/x", "github.com"));
        assert!(matches_site("https://api.github.com/x", "github.com"));
        assert!(!matches_site("https://notgithub.com/x", "github.com"));
        assert!(matches_site(
            "https://github.com/anthropics/x",
            "github.com/anthropics"
        ));
        assert!(!matches_site(
            "https://github.com/other/x",
            "github.com/anthropics"
        ));
    }

    #[test]
    fn the_filetype_matcher_looks_at_the_path() {
        assert!(matches_filetype("https://x.com/a.pdf", "pdf"));
        assert!(!matches_filetype("https://x.com/a.pdf", "doc"));
    }

    #[test]
    fn a_constraint_that_would_empty_the_results_is_dropped() {
        let sources = vec![SearchSource {
            title: "Rust".into(),
            url: "https://rust-lang.org/".into(),
            ..Default::default()
        }];
        let query = parse("rust site:github.com");
        let filtered = apply_query_constraints(&sources, &query);
        assert_eq!(filtered.sources.len(), 1, "the sources are kept");
        assert_eq!(filtered.dropped, vec!["site:github.com"]);
    }

    #[test]
    fn a_constraint_that_matches_keeps_the_sources() {
        let sources = vec![SearchSource {
            title: "Rust".into(),
            url: "https://github.com/rust-lang/rust".into(),
            ..Default::default()
        }];
        let query = parse("rust site:github.com");
        let filtered = apply_query_constraints(&sources, &query);
        assert_eq!(filtered.sources.len(), 1);
        assert!(filtered.dropped.is_empty());
    }

    #[test]
    fn a_source_without_a_date_passes_the_date_bounds() {
        let sources = vec![SearchSource {
            title: "Rust".into(),
            url: "https://rust-lang.org/".into(),
            ..Default::default()
        }];
        let query = parse("rust after:2024-01-01");
        let filtered = apply_query_constraints(&sources, &query);
        assert_eq!(filtered.sources.len(), 1);
        assert!(filtered.dropped.is_empty());
    }
}

//! The shapes of a web search (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner): `web/search/types.ts`
//! and `web/search/utils.ts`.

use std::fmt;

/// One source of a search: a page the model may read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchSource {
    /// The title of the page.
    pub title: String,
    /// The address of the page.
    pub url: String,
    /// The preview text of the page.
    pub snippet: Option<String>,
    /// When the page was published, as the provider gave it.
    pub published_date: Option<String>,
    /// How old the page is, in seconds.
    pub age_seconds: Option<u64>,
}

/// What a search answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchResponse {
    /// The provider that answered.
    pub provider: String,
    /// The sources it found.
    pub sources: Vec<SearchSource>,
}

impl SearchResponse {
    /// Whether there is something to show the model.
    pub fn has_content(&self) -> bool {
        !self.sources.is_empty()
    }
}

/// A failure of one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProviderError {
    /// The id of the provider that failed.
    pub provider: String,
    /// What went wrong, in a sentence the model may read.
    pub message: String,
    /// The HTTP status, when there was one.
    pub status: Option<u16>,
}

impl SearchProviderError {
    /// A failure without a status.
    pub fn new(provider: &str, message: impl Into<String>) -> Self {
        SearchProviderError {
            provider: provider.to_string(),
            message: message.into(),
            status: None,
        }
    }

    /// A failure with a status.
    pub fn with_status(provider: &str, message: impl Into<String>, status: u16) -> Self {
        SearchProviderError {
            provider: provider.to_string(),
            message: message.into(),
            status: Some(status),
        }
    }
}

impl fmt::Display for SearchProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

/// How fresh the results have to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recency {
    /// The last day.
    Day,
    /// The last week.
    Week,
    /// The last month.
    Month,
    /// The last year.
    Year,
}

impl Recency {
    /// The name of the window, as the model writes it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Recency::Day => "day",
            Recency::Week => "week",
            Recency::Month => "month",
            Recency::Year => "year",
        }
    }

    /// The window out of the name the model wrote.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_lowercase().as_str() {
            "day" => Recency::Day,
            "week" => Recency::Week,
            "month" => Recency::Month,
            "year" => Recency::Year,
            _ => return None,
        })
    }

    /// How many days the window holds.
    pub fn days(&self) -> i64 {
        match self {
            Recency::Day => 1,
            Recency::Week => 7,
            Recency::Month => 30,
            Recency::Year => 365,
        }
    }

    /// The lower bound of the window, as an ISO `YYYY-MM-DD`.
    pub fn after_date(&self) -> String {
        let days = super::query::parse_iso_seconds("1970-01-01").unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        let day = (now - self.days() * 86_400) / 86_400;
        let _ = days;
        iso_from_days(day)
    }
}

/// The ISO date of a day, counted from the epoch.
pub fn iso_from_days(days: i64) -> String {
    // The days from the civil date, backwards.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// The age of a date, in seconds.
pub fn date_to_age_seconds(date: &str) -> Option<u64> {
    let seconds = super::query::parse_iso_seconds(date)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    if seconds > now {
        return Some(0);
    }
    Some((now - seconds) as u64)
}

/// Clamp a number of results to `[1, max]`, and use the default when there is
/// no number at all.
pub fn clamp_num_results(value: Option<usize>, default: usize, max: usize) -> usize {
    match value {
        Some(value) if value > 0 => value.min(max).max(1),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_of_results_is_clamped() {
        assert_eq!(clamp_num_results(None, 10, 20), 10);
        assert_eq!(clamp_num_results(Some(0), 10, 20), 10);
        assert_eq!(clamp_num_results(Some(5), 10, 20), 5);
        assert_eq!(clamp_num_results(Some(50), 10, 20), 20);
    }

    #[test]
    fn the_recency_is_read_from_its_name() {
        assert_eq!(Recency::parse("day"), Some(Recency::Day));
        assert_eq!(Recency::parse("YEAR"), Some(Recency::Year));
        assert_eq!(Recency::parse("hour"), None);
    }

    #[test]
    fn the_window_has_a_lower_bound() {
        // A day ago, and not the day before the epoch.
        let bound = Recency::Day.after_date();
        assert_eq!(bound.len(), 10, "{bound}");
        assert!(bound.starts_with("20"), "{bound}");
    }

    #[test]
    fn the_age_of_a_date_is_counted_in_seconds() {
        let age = date_to_age_seconds("2020-01-01T00:00:00Z");
        assert!(age.is_some_and(|age| age > 100 * 86_400), "{age:?}");
        assert!(date_to_age_seconds("not a date").is_none());
    }
}

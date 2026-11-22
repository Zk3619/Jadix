//! Conservative freshness for public, uncredentialed GET representations.
//! Variants and revalidation are deliberately misses, never stale offline hits.
use crate::HttpHeader;
use std::time::{Duration, SystemTime};

/// Remaining freshness after response transfer, including intermediary Age and
/// request delay. Callers must key by the exact URL and bound their own retention.
/// No heuristic lifetime is inferred from Expires or Last-Modified.
pub fn freshness(headers: &[HttpHeader], now: SystemTime, delay: Duration) -> Option<Duration> {
    let mut max_age = None;
    let mut age = None;
    let mut date = None;
    for header in headers {
        let name = header.name.as_str();
        let value = std::str::from_utf8(&header.value).ok()?.trim();
        if name.eq_ignore_ascii_case("cache-control") {
            for directive in value.split(',') {
                let (name, value) = directive
                    .trim()
                    .split_once('=')
                    .map_or((directive.trim(), None), |(name, value)| {
                        (name.trim(), Some(value.trim()))
                    });
                if ["no-store", "no-cache", "private"]
                    .iter()
                    .any(|blocked| name.eq_ignore_ascii_case(blocked))
                {
                    return None;
                }
                if name.eq_ignore_ascii_case("max-age") {
                    let value = value?;
                    let value = if value.starts_with('"') {
                        value.strip_prefix('"')?.strip_suffix('"')?
                    } else {
                        value
                    };
                    if max_age.is_some() {
                        return None;
                    }
                    max_age = Some(seconds(value)?);
                }
            }
        } else if name.eq_ignore_ascii_case("age") {
            if age.is_some() {
                return None;
            }
            age = Some(seconds(value)?);
        } else if name.eq_ignore_ascii_case("date") {
            if date.is_some() {
                return None;
            }
            date = Some(httpdate::parse_http_date(value).ok()?);
        } else if ["vary", "set-cookie", "pragma", "content-range"]
            .iter()
            .any(|blocked| name.eq_ignore_ascii_case(blocked))
        {
            return None;
        }
    }
    let current_age = now
        .duration_since(date?)
        .unwrap_or_default()
        .max(Duration::from_secs(age.unwrap_or(0)).checked_add(delay)?);
    Duration::from_secs(max_age?.min(86400))
        .checked_sub(current_age)
        .filter(|age| !age.is_zero())
}

fn seconds(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(extra: &[(&str, &str)]) -> Vec<HttpHeader> {
        std::iter::once(("Date", "Sun, 06 Nov 1994 08:49:37 GMT"))
            .chain(extra.iter().copied())
            .map(|(name, value)| HttpHeader {
                name: name.into(),
                value: value.into(),
            })
            .collect()
    }
    #[test]
    fn age_and_transfer_delay_reduce_public_freshness_without_extending_stale_data() {
        let now = httpdate::parse_http_date("Sun, 06 Nov 1994 08:49:47 GMT").unwrap();
        assert_eq!(
            freshness(
                &headers(&[("Cache-Control", "public, max-age=60"), ("Age", "12")]),
                now,
                Duration::from_secs(3)
            ),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            freshness(
                &headers(&[("Cache-Control", "max-age=10")]),
                now,
                Duration::ZERO
            ),
            None
        );
        assert_eq!(
            freshness(
                &headers(&[("Cache-Control", "max-age=60"), ("Age", "59")]),
                now,
                Duration::from_secs(2)
            ),
            None
        );
    }
    #[test]
    fn variants_privacy_and_ambiguous_or_missing_freshness_are_never_cached() {
        let now = httpdate::parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        for extra in [
            vec![("Cache-Control", "max-age=60, no-store")],
            vec![("Cache-Control", "max-age=60, PRIVATE")],
            vec![("Cache-Control", "max-age=60, no-cache")],
            vec![("Cache-Control", "max-age=60, max-age=61")],
            vec![("Cache-Control", "max-age=-1")],
            vec![("Cache-Control", "max-age=60"), ("Vary", "Accept")],
            vec![("Cache-Control", "max-age=60"), ("Set-Cookie", "a=b")],
            vec![("Cache-Control", "max-age=60"), ("Age", "1"), ("Age", "2")],
            vec![("Cache-Control", "max-age=60"), ("Date", "invalid")],
            vec![("Expires", "Sun, 06 Nov 1994 08:59:37 GMT")],
        ] {
            assert_eq!(
                freshness(&headers(&extra), now, Duration::ZERO),
                None,
                "{extra:?}"
            );
        }
        assert_eq!(
            freshness(
                &[HttpHeader {
                    name: "Cache-Control".into(),
                    value: "max-age=60".into()
                }],
                now,
                Duration::ZERO
            ),
            None
        );
        assert_eq!(
            freshness(
                &headers(&[("Cache-Control", "max-age=\"60\"")]),
                now,
                Duration::ZERO
            ),
            Some(Duration::from_secs(60))
        );
    }
}

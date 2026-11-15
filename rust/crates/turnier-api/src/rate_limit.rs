use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::StatusCode;

use crate::error::{WebError, WebResult};

const HOUR: Duration = Duration::from_secs(60 * 60);
pub const HOURLY_LOG_MAX_SOURCES: usize = 10_000;

pub type HourlyLog = Mutex<HashMap<IpAddr, Vec<Instant>>>;

pub fn limiter_key(ip: IpAddr) -> IpAddr {
    let IpAddr::V6(v6) = ip else {
        return ip;
    };
    if let Some(v4) = v6.to_ipv4_mapped() {
        return IpAddr::V4(v4);
    }
    let s = v6.segments();
    IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
}

pub fn enforce_hourly_limit(
    log: &HourlyLog,
    ip: IpAddr,
    limit: usize,
    message: impl FnOnce() -> String,
) -> WebResult<()> {
    let mut log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    record_hourly(&mut log, ip, limit, Instant::now(), message)
}

pub fn record_hourly(
    log: &mut HashMap<IpAddr, Vec<Instant>>,
    ip: IpAddr,
    limit: usize,
    now: Instant,
    message: impl FnOnce() -> String,
) -> WebResult<()> {
    let key = limiter_key(ip);
    if log.len() >= HOURLY_LOG_MAX_SOURCES && !log.contains_key(&key) {
        log.retain(|_, times| {
            times.retain(|at| now.duration_since(*at) < HOUR);
            !times.is_empty()
        });
        if log.len() >= HOURLY_LOG_MAX_SOURCES {
            return Err(WebError::new(StatusCode::TOO_MANY_REQUESTS, message()));
        }
    }
    let entries = log.entry(key).or_default();
    entries.retain(|at| now.duration_since(*at) < HOUR);
    if entries.len() >= limit {
        return Err(WebError::new(StatusCode::TOO_MANY_REQUESTS, message()));
    }
    entries.push(now);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_addresses_share_their_64_prefix() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(limiter_key(a), limiter_key(b));
        assert_ne!(limiter_key(a), limiter_key(other));
        let mapped: IpAddr = "::ffff:192.0.2.7".parse().unwrap();
        assert_eq!(limiter_key(mapped), "192.0.2.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn limit_counts_per_prefix() {
        let log = HourlyLog::default();
        let a: IpAddr = "2001:db8::1".parse().unwrap();
        let b: IpAddr = "2001:db8::2".parse().unwrap();
        assert!(enforce_hourly_limit(&log, a, 1, String::new).is_ok());
        assert!(enforce_hourly_limit(&log, b, 1, String::new).is_err());
    }
}

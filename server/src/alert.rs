use std::time::{Duration, Instant};

pub const TEMP_ALERT_COOLDOWN: Duration = Duration::from_secs(10 * 60);
pub const TEMP_ALERT_MIN: u8 = 70;
pub const TEMP_ALERT_MAX: u8 = 97;
pub const TEMP_ALERT_DEFAULT: u8 = 90;

pub fn clamp_threshold(celsius: u8) -> u8 {
    celsius.clamp(TEMP_ALERT_MIN, TEMP_ALERT_MAX)
}

pub fn should_fire_temp_alert(
    enabled: bool,
    threshold: u8,
    temp: u8,
    last_alert: Option<Instant>,
    now: Instant,
) -> bool {
    if !enabled || temp < clamp_threshold(threshold) {
        return false;
    }
    match last_alert {
        None => true,
        Some(previous) => now.saturating_duration_since(previous) >= TEMP_ALERT_COOLDOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_fire_when_disabled() {
        let now = Instant::now();
        assert!(!should_fire_temp_alert(false, 90, 95, None, now));
    }

    #[test]
    fn does_not_fire_when_cool() {
        let now = Instant::now();
        assert!(!should_fire_temp_alert(true, 90, 80, None, now));
    }

    #[test]
    fn fires_when_hot_and_no_previous() {
        let now = Instant::now();
        assert!(should_fire_temp_alert(true, 90, 90, None, now));
    }

    #[test]
    fn does_not_fire_within_cooldown() {
        let now = Instant::now();
        let previous = now - Duration::from_secs(60);
        assert!(!should_fire_temp_alert(true, 90, 95, Some(previous), now));
    }

    #[test]
    fn fires_after_cooldown() {
        let now = Instant::now();
        let previous = now - TEMP_ALERT_COOLDOWN;
        assert!(should_fire_temp_alert(true, 90, 91, Some(previous), now));
    }
}

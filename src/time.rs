use chrono::{Timelike, Utc};
use tokio::time::Instant;

/// Return an Instant that approximately represents the next `minute % 5 == 0` of the current hour
pub(crate) fn instant_at_minute() -> Instant {
    Instant::now()
        .checked_add(
            chrono::Duration::minutes((5 - Utc::now().minute() % 5) as i64)
                .to_std()
                .unwrap(),
        )
        .unwrap()
}

/// Return a String representation of the calculated time
pub(crate) fn future_point_as_hh_mm() -> String {
    let duration = duration_since_now();
    Utc::now()
        .checked_add_signed(chrono::Duration::from_std(duration).unwrap())
        .unwrap()
        .format("%H:%M")
        .to_string()
}

/// Take an Instant and calculate the Duration between that Instant and "now"
fn duration_since_now() -> std::time::Duration {
    let instant = instant_at_minute();
    instant.duration_since(Instant::now())
}

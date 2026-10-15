use serde::{Deserialize, Serialize};

#[cfg(feature = "desktop")]
use std::time;

#[cfg(feature = "web")]
use web_time as time;

// ----------------------------------------------
// Type Aliases
// ----------------------------------------------

pub type Instant  = time::Instant;
pub type Duration = time::Duration;

pub type Seconds = f32;
pub type Milliseconds = f32;

#[inline]
pub fn elapsed_seconds(a: Instant, b: Instant) -> Seconds {
    let elapsed = a - b;
    elapsed.as_secs_f32()
}

// ----------------------------------------------
// FrameClock
// ----------------------------------------------

pub struct FrameClock {
    last_frame_time: Instant,
    delta_time: Duration,

    // When set, delta_time() reports this value every frame instead of the measured frame
    // time, so frame-driven timers advance the same way on every run regardless of frame rate.
    fixed_delta_time: Option<Seconds>,

    // Set by a replay (--replay) to the recorded run's time step for the frame. Wins over the
    // fixed delta.
    replay_delta_time: Option<Seconds>,

    // Index of the current frame, starting at 0. Advances in end_frame().
    frame_number: u64,
}

impl FrameClock {
    #[inline]
    pub fn new() -> Self {
        Self {
            last_frame_time: Instant::now(),
            delta_time: Duration::ZERO,
            fixed_delta_time: None,
            replay_delta_time: None,
            frame_number: 0,
        }
    }

    #[inline]
    pub fn begin_frame(&self) {}

    #[inline]
    pub fn end_frame(&mut self) {
        let time_now = Instant::now();
        self.delta_time = time_now - self.last_frame_time;
        self.last_frame_time = time_now;
        self.frame_number += 1;
    }

    // Time step to advance the game by this frame: the replayed one during a replay, else the
    // fixed delta when one is set, otherwise the measured duration of the previous frame.
    #[inline]
    #[must_use]
    pub fn delta_time(&self) -> Seconds {
        self.replay_delta_time.or(self.fixed_delta_time).unwrap_or_else(|| self.real_delta_time())
    }

    // Measured duration of the previous frame, ignoring any fixed delta (for frame rate display).
    #[inline]
    #[must_use]
    pub fn real_delta_time(&self) -> Seconds {
        self.delta_time.as_secs_f32()
    }

    #[inline]
    pub fn set_fixed_delta_time(&mut self, fixed_delta_time: Option<Seconds>) {
        debug_assert!(fixed_delta_time.is_none_or(|dt| dt > 0.0), "Fixed delta time must be positive!");
        self.fixed_delta_time = fixed_delta_time;
    }

    #[inline]
    #[must_use]
    pub fn fixed_delta_time(&self) -> Option<Seconds> {
        self.fixed_delta_time
    }

    #[inline]
    pub fn set_replay_delta_time(&mut self, replay_delta_time: Option<Seconds>) {
        self.replay_delta_time = replay_delta_time;
    }

    #[inline]
    #[must_use]
    pub fn frame_number(&self) -> u64 {
        self.frame_number
    }
}

// ----------------------------------------------
// UpdateTimer
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct UpdateTimer {
    #[serde(skip, default = "default_timer_update_frequency")]
    pub update_frequency_secs: Seconds,
    time_since_last_update_secs: Seconds,
}

#[inline]
const fn default_timer_update_frequency() -> Seconds {
    Seconds::INFINITY
}

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UpdateTimerResult {
    DoNotUpdate,
    ShouldUpdate,
}

impl UpdateTimerResult {
    #[inline]
    pub fn should_update(self) -> bool {
        self == UpdateTimerResult::ShouldUpdate
    }
}

impl UpdateTimer {
    #[inline]
    pub fn new(update_frequency_secs: Seconds) -> Self {
        Self { update_frequency_secs, time_since_last_update_secs: 0.0 }
    }

    #[inline]
    pub fn tick(&mut self, delta_time_secs: Seconds) -> UpdateTimerResult {
        // If we hit any of these, there's a missing pos_load() call after
        // deserialization.
        debug_assert!(self.update_frequency_secs.is_finite());
        debug_assert!(self.time_since_last_update_secs.is_finite());

        if self.time_since_last_update_secs >= self.update_frequency_secs {
            // Reset the clock.
            self.time_since_last_update_secs = 0.0;
            UpdateTimerResult::ShouldUpdate
        } else {
            // Advance the clock.
            self.time_since_last_update_secs += delta_time_secs;
            UpdateTimerResult::DoNotUpdate
        }
    }

    #[inline]
    pub fn frequency_secs(&self) -> f32 {
        self.update_frequency_secs
    }

    #[inline]
    pub fn time_since_last_secs(&self) -> f32 {
        self.time_since_last_update_secs
    }

    #[inline]
    pub fn reset(&mut self) {
        self.time_since_last_update_secs = 0.0;
    }

    #[inline]
    pub fn force_update(&mut self) {
        self.time_since_last_update_secs = self.update_frequency_secs;
    }

    #[inline]
    pub fn post_load(&mut self, update_frequency_secs: Seconds) {
        debug_assert!(update_frequency_secs.is_finite());
        self.update_frequency_secs = update_frequency_secs;
    }
}

// ----------------------------------------------
// CountdownTimer
// ----------------------------------------------

// Triggers an event when countdown reaches zero.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CountdownTimer {
    countdown: Seconds,
}

impl CountdownTimer {
    #[inline]
    pub fn new(countdown: Seconds) -> Self {
        Self { countdown }
    }

    #[inline]
    pub fn reset(&mut self, countdown: Seconds) {
        self.countdown = countdown;
    }

    #[inline]
    pub fn remaining_secs(&self) -> Seconds {
        self.countdown
    }

    #[inline]
    pub fn tick(&mut self, delta_time_secs: Seconds) -> bool {
        self.countdown -= delta_time_secs;
        if self.countdown <= 0.0 {
            self.countdown = 0.0;
            true
        } else {
            false
        }
    }
}

// ----------------------------------------------
// PerfTimer
// ----------------------------------------------

pub struct PerfTimer(Instant);

impl PerfTimer {
    #[inline]
    pub fn begin() -> Self {
        PerfTimer(Instant::now())
    }

    #[inline]
    #[must_use]
    pub fn end(self) -> Milliseconds {
        self.0.elapsed().as_secs_f32() * 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_clock_fixed_delta() {
        let mut clock = FrameClock::new();
        assert_eq!(clock.fixed_delta_time(), None);

        clock.set_fixed_delta_time(Some(1.0 / 60.0));
        assert_eq!(clock.fixed_delta_time(), Some(1.0 / 60.0));

        // The fixed delta is reported regardless of how long frames actually take.
        for _ in 0..3 {
            clock.begin_frame();
            std::thread::sleep(Duration::from_millis(2));
            clock.end_frame();
            assert_eq!(clock.delta_time(), 1.0 / 60.0);
        }

        // The measured frame time is still available.
        assert!(clock.real_delta_time() > 0.0);

        // Clearing the fixed delta goes back to the measured frame time.
        clock.set_fixed_delta_time(None);
        assert_eq!(clock.delta_time(), clock.real_delta_time());
    }

    #[test]
    fn test_frame_clock_frame_number() {
        let mut clock = FrameClock::new();
        assert_eq!(clock.frame_number(), 0);

        for expected in 1..=5 {
            clock.begin_frame();
            clock.end_frame();
            assert_eq!(clock.frame_number(), expected);
        }
    }
}

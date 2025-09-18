use std::time;
use serde::{Deserialize, Serialize};
use crate::imgui_ui::UiSystem;

// ----------------------------------------------
// FrameClock
// ----------------------------------------------

pub type Seconds = f32;

pub struct FrameClock {
    last_frame_time: time::Instant,
    delta_time: time::Duration,
}

impl FrameClock {
    #[inline]
    pub fn new() -> Self {
        Self { last_frame_time: time::Instant::now(), delta_time: time::Duration::new(0, 0) }
    }

    #[inline]
    pub fn begin_frame(&self) {}

    #[inline]
    pub fn end_frame(&mut self) {
        let time_now = time::Instant::now();
        self.delta_time = time_now - self.last_frame_time;
        self.last_frame_time = time_now;
    }

    #[inline]
    #[must_use]
    pub fn delta_time(&self) -> Seconds {
        self.delta_time.as_secs_f32()
    }
}

// ----------------------------------------------
// UpdateTimer
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct UpdateTimer {
    update_frequency_secs: Seconds,
    time_since_last_update_secs: Seconds,
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

    pub fn draw_debug_ui(&mut self, label: &str, imgui_id: u32, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        ui.text(format!("{}:", label));

        ui.input_float(format!("Frequency (secs)##_timer_frequency_{}", imgui_id),
                       &mut self.update_frequency_secs)
          .display_format("%.2f")
          .step(0.5)
          .build();

        ui.input_float(format!("Time since last##_last_update_{}", imgui_id),
                       &mut self.time_since_last_update_secs)
          .display_format("%.2f")
          .read_only(true)
          .build();
    }
}

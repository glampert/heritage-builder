use common::{coords::Cell, time::Seconds};
use game::{
    config::GameConfigs,
    sim::{
        calendar::{CalendarDate, CalendarEvent},
        resources::ResourceKind,
        SimCmdQueue,
    },
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for game time: the simulation's
// world step at different speeds, and the calendar
// it drives.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Game Time", &[
        test_utils::test_fn!(test_world_steps_add_up_to_simulated_time),
        test_utils::test_fn!(test_building_timers_follow_simulated_time),
        test_utils::test_fn!(test_calendar_follows_simulated_time),
    ]);
}

const FRAME_SECS: Seconds = 1.0 / 60.0;

// What a run at `speed` saw: the simulated time the world steps covered, and the calendar
// events they crossed.
struct Run {
    world_secs: f64,
    events: Vec<CalendarEvent>,
}

// Runs frames at `speed` until `sim_secs` of simulated time have passed, the way
// Simulation::update steps the world, with the world and its buildings updated on each step.
fn run_at_speed(env: &mut TestEnvironment, speed: f32, sim_secs: f64) -> Run {
    env.sim.set_speed(speed);
    assert_eq!(env.sim.speed(), speed);

    let frames = (sim_secs / f64::from(FRAME_SECS * speed)).round() as usize;
    let mut run = Run { world_secs: 0.0, events: Vec::new() };

    for _ in 0..frames {
        let Some(step) = env.sim.advance_clocks(FRAME_SECS * env.sim.speed()) else {
            continue;
        };

        run.world_secs += f64::from(step.delta_time_secs);
        run.events.extend(step.calendar_events.iter().copied());

        let context = env.new_sim_context(step.delta_time_secs);
        let cmds = context.cmds_mut();
        context.world_mut().update(cmds, &context);
        cmds.execute(&context);
    }

    run
}

// The world step is the simulated time since the last one: frame time times the speed, once.
// It used to be multiplied by the speed twice, so the world ran at speed squared.
fn test_world_steps_add_up_to_simulated_time() {
    const SIM_SECS: f64 = 120.0;

    for speed in [1.0, 2.0, 10.0] {
        let mut env = TestEnvironment::new();
        let run = run_at_speed(&mut env, speed, SIM_SECS);

        // The last step may be partway: up to one update period (plus a frame) short.
        let update_period = f64::from(GameConfigs::get().sim.update_frequency_secs);
        let max_shortfall = update_period + f64::from(FRAME_SECS * speed);

        assert!(
            run.world_secs <= env.sim.elapsed_secs() + 1e-3 && run.world_secs >= SIM_SECS - max_shortfall,
            "at {speed}x the world steps covered {:.3}s of {SIM_SECS}s simulated ({:.3}s by elapsed_secs)",
            run.world_secs,
            env.sim.elapsed_secs()
        );
    }
}

// A rice farm's production timer fills its stock at the same rate per simulated second at any speed.
// It makes 2 rice about every 19 s (18 s and a world step), up to 8: 66 s is three cycles, with
// time to spare on both sides, and a farm running at twice the rate would reach the cap.
fn test_building_timers_follow_simulated_time() {
    const SIM_SECS: f64 = 66.0;
    const EXPECTED_RICE: u32 = 6;

    let rice_after = |speed: f32| {
        let mut env = TestEnvironment::new();
        let farm = test_utils::spawn_building(&mut env, Cell::new(8, 8), "rice_farm");

        run_at_speed(&mut env, speed, SIM_SECS);
        test_utils::find_building(&env, farm).available_resources(ResourceKind::Rice)
    };

    for speed in [1.0, 2.0, 10.0] {
        let rice = rice_after(speed);
        assert_eq!(rice, EXPECTED_RICE, "{SIM_SECS} simulated seconds made {rice} rice at {speed}x");
    }
}

// The calendar advances by the world steps, a month every seconds_per_month, at any speed.
fn test_calendar_follows_simulated_time() {
    let configs = &GameConfigs::get().sim;
    let months = 13;
    let sim_secs = f64::from(configs.seconds_per_month) * f64::from(months) + 1.0;

    for speed in [1.0, 2.0, 10.0] {
        let mut env = TestEnvironment::new();
        let start = env.sim.calendar().date(configs.days_per_month, configs.seconds_per_month);
        let run = run_at_speed(&mut env, speed, sim_secs);

        assert_eq!(env.sim.calendar().months_elapsed(), months, "months elapsed at {speed}x");

        let date = env.sim.calendar().date(configs.days_per_month, configs.seconds_per_month);
        assert_eq!(date.ordinal_month(), start.ordinal_month() + months);

        let new_months = run.events.iter().filter(|event| matches!(event, CalendarEvent::NewMonth(_))).count();
        assert_eq!(new_months, months as usize, "new month events at {speed}x");

        let new_year = CalendarEvent::NewYear(start.year + 1);
        assert!(run.events.contains(&new_year), "a new year at {speed}x: {:?}", run.events);

        let first_month = CalendarDate { year: start.year, month: start.month + 1, day: 1 };
        assert_eq!(run.events.first(), Some(&CalendarEvent::NewMonth(first_month)));
    }
}

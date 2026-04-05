// NOTE: Allow for the whole crate.
#![allow(dead_code)]

use engine::runner;
use game::GameLoop;

fn main() {
    runner::run::<GameLoop>();
}

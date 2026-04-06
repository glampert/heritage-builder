// Launcher binary crate - defines the main() entry point for the application.
// Compiles to the "HeritageBuilder" executable.

use engine::runner;
use game::GameLoop;

fn main() {
    runner::run::<GameLoop>();
}

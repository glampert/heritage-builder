use engine::{
    log,
    render::texture::TextureCache,
};
use game::{
    cheats,
    tile::sets::TileSets,
    config::GameConfigs,
    unit::config::UnitConfigs,
    prop::config::PropConfigs,
    building::config::BuildingConfigs,
};

// One-time environment setup. Must be called on the main thread
// before any tests, since the game uses SingleThreadStatic globals
// that assert access from the thread that initialized them.
fn setup() {
    log::info!("-------------------------");
    log::info!("  Starting Test Harness  ");
    log::info!("-------------------------");

    GameConfigs::load();
    UnitConfigs::load();
    PropConfigs::load();
    BuildingConfigs::load();

    cheats::initialize();
    cheats::get_mut().ignore_tile_cost = true; // So we can spawn anything...

    let mut tex_cache = TextureCache::default();
    let skip_loading_textures = true;
    TileSets::load(&mut tex_cache, false, false, skip_loading_textures);
}

// Runs all tests sequentially on the calling thread.
// Calls setup() once before running any tests.
pub fn run_tests(tests: &[(&str, fn())]) {
    setup();

    let mut passed = 0;
    let mut failed = 0;

    for (name, test_fn) in tests {
        print!("test {name} ... ");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test_fn));
        match result {
            Ok(()) => { println!("ok"); passed += 1; }
            Err(_) => { println!("FAILED"); failed += 1; }
        }
    }

    println!("\ntest result: {} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

macro_rules! test_fn {
    ($f:ident) => {
        (stringify!($f), $f)
    };
}

pub(crate) use test_fn;

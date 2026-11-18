// The river's floods (docs/plans/seasons-floods.md): what the World Debug window and automation ask
// for, run where the world and the tile map can change. The state is sim::floods::CityFloods.

use common::mem::SingleThreadStatic;

use crate::{config::GameConfigs, sim::SimContextMut};

// What the World Debug window's Floods tab asks for, run at the start of the next frame
// (Simulation::update): the window draws while the world is borrowed.
#[derive(Copy, Clone)]
pub enum Request {
    SetEnabled(bool),
}

static REQUESTS: SingleThreadStatic<Vec<Request>> = SingleThreadStatic::new(Vec::new());

pub fn request(request: Request) {
    REQUESTS.as_mut().push(request);
}

#[inline]
pub fn has_requests() -> bool {
    !REQUESTS.as_ref().is_empty()
}

pub fn run_requests(context: &SimContextMut) {
    for request in std::mem::take(REQUESTS.as_mut()) {
        match request {
            Request::SetEnabled(enabled) => {
                context.floods_mut().set_enabled(enabled, &GameConfigs::get().sim.floods, context.tile_map());
            }
        }
    }
}

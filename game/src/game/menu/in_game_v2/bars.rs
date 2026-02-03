use std::{any::Any, rc::Rc};
use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter};

use crate::{
    game::menu::GameMenusInputArgs,
    utils::mem::{RcMut, WeakMut, WeakRef},
    ui::{UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// InGameMenuBars
// ----------------------------------------------

pub struct InGameMenuBars {
    bars: ArrayVec<RcMut<dyn MenuBar>, MENU_BAR_COUNT>,
}

pub type InGameMenuBarsRcMut   = RcMut<InGameMenuBars>;
pub type InGameMenuBarsWeakMut = WeakMut<InGameMenuBars>;
pub type InGameMenuBarsWeakRef = WeakRef<InGameMenuBars>;

impl InGameMenuBars {
    pub fn new(context: &mut UiWidgetContext) -> InGameMenuBarsRcMut {
        let mut bars = ArrayVec::new();

        for bar_kind in MenuBarKind::iter() {
            bars.push(bar_kind.build(context));
        }

        InGameMenuBarsRcMut::new(Self { bars })
    }

    pub fn handle_input(&mut self, context: &mut UiWidgetContext, args: GameMenusInputArgs) -> UiInputEvent {
        for bar in &mut self.bars {
            let input_event = bar.handle_input(context, args);
            if input_event.is_handled() {
                return input_event;
            }
        }
        UiInputEvent::NotHandled
    }

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        for bar in &mut self.bars {
            bar.draw(context);
        }
    }
}

// ----------------------------------------------
// MenuBarKind
// ----------------------------------------------

const MENU_BAR_COUNT: usize = MenuBarKind::COUNT;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumIter)]
enum MenuBarKind {
    Top,
    Left,
    SpeedControls,
}

impl MenuBarKind {
    fn build(self, context: &mut UiWidgetContext) -> RcMut<dyn MenuBar> {
        match self {
            Self::Top => TopBar::build(context),
            Self::Left => LeftBar::build(context),
            Self::SpeedControls => SpeedControlsBar::build(context),
        }
    }
}

// ----------------------------------------------
// MenuBar
// ----------------------------------------------

trait MenuBar: Any {
    fn as_any(&self) -> &dyn Any;
    fn kind(&self) -> MenuBarKind;
    fn draw(&mut self, context: &mut UiWidgetContext);
    fn handle_input(&mut self, _context: &mut UiWidgetContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }
}

// ----------------------------------------------
// TopBar
// ----------------------------------------------

struct TopBar {
}

impl MenuBar for TopBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> MenuBarKind {
        MenuBarKind::Top
    }

    fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}

impl TopBar {
    fn build(_context: &mut UiWidgetContext) -> RcMut<dyn MenuBar> {
        let instance: Rc<dyn MenuBar> = Rc::new(Self {
        });
        RcMut::from(instance)
    }
}

// ----------------------------------------------
// LeftBar
// ----------------------------------------------

struct LeftBar {
}

impl MenuBar for LeftBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> MenuBarKind {
        MenuBarKind::Left
    }

    fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}

impl LeftBar {
    fn build(_context: &mut UiWidgetContext) -> RcMut<dyn MenuBar> {
        let instance: Rc<dyn MenuBar> = Rc::new(Self {
        });
        RcMut::from(instance)
    }
}

// ----------------------------------------------
// SpeedControlsBar
// ----------------------------------------------

struct SpeedControlsBar {
}

impl MenuBar for SpeedControlsBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> MenuBarKind {
        MenuBarKind::SpeedControls
    }

    fn draw(&mut self, _context: &mut UiWidgetContext) {
    }
}

impl SpeedControlsBar {
    fn build(_context: &mut UiWidgetContext) -> RcMut<dyn MenuBar> {
        let instance: Rc<dyn MenuBar> = Rc::new(Self {
        });
        RcMut::from(instance)
    }
}

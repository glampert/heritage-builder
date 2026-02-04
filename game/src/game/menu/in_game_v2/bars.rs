use std::{any::Any, rc::Rc};
use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    engine::time::Seconds,
    ui::{UiInputEvent, widgets::*},
    utils::{Vec2, mem::{RcMut, WeakMut, WeakRef}},
    game::menu::{
        GameMenusInputArgs, ButtonDef,
        TOOLTIP_FONT_SCALE, SMALL_VERTICAL_SEPARATOR_SPRITE,
    },
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
            bars.push(bar_kind.build_menu(context));
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
    fn build_menu(self, context: &mut UiWidgetContext) -> RcMut<dyn MenuBar> {
        let rc: Rc<dyn MenuBar> = {
            match self {
                Self::Top => TopBar::new(context),
                Self::Left => LeftBar::new(context),
                Self::SpeedControls => SpeedControlsBar::new(context),
            }
        };
        RcMut::from(rc)
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
    // TODO / WIP
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
    fn new(_context: &mut UiWidgetContext) -> Rc<TopBar> {
        Rc::new(Self {
        })
    }
}

// ----------------------------------------------
// LeftBar
// ----------------------------------------------

struct LeftBar {
    // TODO / WIP
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
    fn new(_context: &mut UiWidgetContext) -> Rc<LeftBar> {
        Rc::new(Self {
        })
    }
}

// ----------------------------------------------
// SpeedControlsButtonKind
// ----------------------------------------------

const SPEED_CONTROLS_BUTTON_SIZE: Vec2 = Vec2::new(20.0, 20.0);
const SPEED_CONTROLS_BUTTON_SPACING: f32 = 2.0;

const SPEED_CONTROLS_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED: bool = true;
const SPEED_CONTROLS_BUTTON_STATE_TRANSITION_SECS: Seconds = 0.5;

const SPEED_CONTROLS_BUTTON_COUNT: usize = SpeedControlsButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum SpeedControlsButtonKind {
    #[strum(props(Label = "menu_bar/play"))]
    Play,

    #[strum(props(Label = "menu_bar/pause"))]
    Pause,

    #[strum(props(Label = "menu_bar/slowdown"))]
    Slowdown,

    #[strum(props(Label = "menu_bar/speedup"))]
    Speedup,
}

impl ButtonDef for SpeedControlsButtonKind {}

// ----------------------------------------------
// SpeedControlsBar
// ----------------------------------------------

struct SpeedControlsBar {
    current_sim_state: SimState,
    menu: UiMenuRcMut,
}

impl MenuBar for SpeedControlsBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> MenuBarKind {
        MenuBarKind::SpeedControls
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        let sim_state = SimState::new(context);

        if self.current_sim_state != sim_state {
            self.update_sim_state(sim_state);
        }

        self.menu.draw(context);
    }
}

impl SpeedControlsBar {
    fn new(context: &mut UiWidgetContext) -> Rc<SpeedControlsBar> {
        let mut group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: SPEED_CONTROLS_BUTTON_SPACING,
                center_horizontally: false, // Let content float left.
                stack_vertically: false, // Layout buttons side-by-side.
                ..Default::default()
            }
        );

        for button_kind in SpeedControlsButtonKind::iter() {
            let on_button_state_changed = UiSpriteButtonStateChanged::with_closure(
                move |button, context, _| {
                    if button.is_pressed() {
                        match button_kind {
                            SpeedControlsButtonKind::Play => {
                                context.sim.resume();
                            }
                            SpeedControlsButtonKind::Pause => {
                                context.sim.pause();
                            }
                            SpeedControlsButtonKind::Slowdown => {
                                context.sim.resume();
                                context.sim.slowdown();
                            }
                            SpeedControlsButtonKind::Speedup => {
                                context.sim.resume();
                                context.sim.speedup();
                            }
                        }

                        // Pressed state doesn't persist.
                        button.press(false);
                    }
                }
            );

            let button = button_kind.new_sprite_button(
                context,
                SPEED_CONTROLS_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED,
                SPEED_CONTROLS_BUTTON_SIZE,
                SPEED_CONTROLS_BUTTON_STATE_TRANSITION_SECS,
                UiSpriteButtonState::Idle,
                on_button_state_changed
            );

            group.add_widget(button);
        }

        const SEPARATOR_THICKNESS: f32 = 8.0;
        let separator = UiSeparator::new(
            context,
            UiSeparatorParams {
                separator: Some(SMALL_VERTICAL_SEPARATOR_SPRITE),
                thickness: Some(SEPARATOR_THICKNESS),
                vertical: true,
                ..Default::default()
            }
        );

        group.add_widget(separator);

        let sim_state = SimState::new(context);

        let heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                font_scale: TOOLTIP_FONT_SCALE,
                lines: vec![sim_state.to_string()],
                center_horizontally: false, // Let the text float left.
                ..Default::default()
            }
        );

        group.add_widget(heading);

        fn calc_menu_size(context: &UiWidgetContext) -> Vec2 {
            let style = context.ui_sys.current_ui_style();

            let mut padding = Vec2::new(5.0, 0.0); // Add 5px width padding for the label.
            padding += Vec2::from_array(style.window_padding) * 2.0;
            padding += Vec2::new(style.window_border_size, style.window_border_size) * 2.0;

            // Length of longest label: "Paused"
            let label_size = context.calc_text_size(TOOLTIP_FONT_SCALE, &SimState::Paused.to_string());

            let menu_width =
                (SPEED_CONTROLS_BUTTON_SIZE.x  * SPEED_CONTROLS_BUTTON_COUNT as f32) +
                (SPEED_CONTROLS_BUTTON_SPACING * SPEED_CONTROLS_BUTTON_COUNT as f32) +
                SEPARATOR_THICKNESS +
                label_size.x +
                padding.x;

            let menu_height =
                label_size.y.max(SPEED_CONTROLS_BUTTON_SIZE.y) +
                padding.y;

            Vec2::new(menu_width, menu_height)
        }

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("SpeedControlsBar".into()),
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignLeft,
                size: Some(calc_menu_size(context)), // Fixed size menu.
                widget_spacing: Some(Vec2::new(SPEED_CONTROLS_BUTTON_SPACING, SPEED_CONTROLS_BUTTON_SPACING)),
                background: Some("misc/wide_page_bg.png"),
                ..Default::default()
            }
        );

        menu.add_widget(group);

        Rc::new(Self { current_sim_state: sim_state, menu })
    }

    fn update_sim_state(&mut self, sim_state: SimState) {
        self.current_sim_state = sim_state;

        let (_, group) = self.menu
            .find_widget_of_type_mut::<UiWidgetGroup>()
            .unwrap();

        let (_, heading) = group
            .find_widget_of_type_mut::<UiMenuHeading>()
            .unwrap();

        let lines = heading.lines_mut();
        lines[0] = sim_state.to_string();
    }
}

// ----------------------------------------------
// SimState
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq)]
enum SimState {
    Paused,
    Playing(f32),
}

impl SimState {
    fn new(context: &UiWidgetContext) -> Self {
        if context.sim.is_paused() {
            Self::Paused
        } else {
            Self::Playing(context.sim.speed())
        }
    }
}

impl std::fmt::Display for SimState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Paused => write!(f, "Paused"),
            Self::Playing(speed) => write!(f, "{:.0}x", speed),
        }
    }
}

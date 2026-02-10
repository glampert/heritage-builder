use std::{rc::Rc, path::PathBuf};

use arrayvec::ArrayVec;
use num_enum::TryFromPrimitive;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter, Display};

use crate::{
    engine::time::Seconds,
    ui::{self, widgets::*},
    utils::{Vec2, mem::{RcMut, WeakMut, WeakRef}},
    game::menu::{
        ButtonDef,
        dialog::{self, DialogMenuKind},
        TOOLTIP_FONT_SCALE,
        TOOLTIP_BACKGROUND_SPRITE,
        SMALL_VERTICAL_SEPARATOR_SPRITE,
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

trait MenuBar {
    fn draw(&mut self, context: &mut UiWidgetContext);
}

// ----------------------------------------------
// TopBarIcon
// ----------------------------------------------

const TOP_BAR_ICON_SPACING: f32 = 0.0;
const TOP_BAR_ICON_COUNT: usize = TopBarIcon::COUNT;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter, Display, TryFromPrimitive)]
enum TopBarIcon {
    #[strum(props(
        AssetPath = "icons/population_icon.png",
        ClipToMenu = true,
        WithTooltip = true,
        Width = 35,
        Height = 20
    ))]
    Population,

    #[strum(props(
        AssetPath = "icons/player_icon.png",
        ClipToMenu = false, // Player icon overflows the menu bar.
        WithTooltip = false,
        Width = 45,
        Height = 20,
        HeightUnclipped = 45,
        MarginTop = -6,
    ))]
    Player,

    #[strum(props(
        AssetPath = "icons/gold_icon.png",
        ClipToMenu = true,
        WithTooltip = true,
        Width = 35,
        Height = 20
    ))]
    Gold,
}

impl TopBarIcon {
    fn asset_path(self) -> PathBuf {
        // ui/icons/{sprite}.png
        let path = self.get_str("AssetPath").unwrap();
        ui::assets_path().join(path)
    }

    fn clip_to_menu(self) -> bool {
        self.get_bool("ClipToMenu").unwrap()
    }

    fn with_tooltip(self) -> bool {
        self.get_bool("WithTooltip").unwrap()
    }

    fn size(self) -> Vec2 {
        let width  = self.get_int("Width").unwrap()  as f32;
        let height = self.get_int("Height").unwrap() as f32;
        Vec2::new(width, height)
    }

    fn size_unclipped(self) -> Vec2 {
        let mut size = self.size();
        if let Some(width) = self.get_int("WidthUnclipped") {
            size.x = width as f32;
        }
        if let Some(height) = self.get_int("HeightUnclipped") {
            size.y = height as f32;
        }
        size
    }

    fn margin_top(self) -> f32 {
        self.get_int("MarginTop").unwrap_or_default() as f32
    }

    fn label_for_stats(self, stats: &TopBarStats) -> Option<String> {
        match self {
            Self::Population => Some(stats.population.to_string()),
            Self::Gold       => Some(stats.gold.to_string()),
            Self::Player     => None,
        }
    }

    fn max_label_size(context: &UiWidgetContext) -> Vec2 {
        const PLACEHOLDER_LABEL: &str = "0000000"; // Estimate max 7 digits label.
        let mut size = context.calc_text_size(TOOLTIP_FONT_SCALE, PLACEHOLDER_LABEL);
        size.y += 5.0; // explicit vertical padding.
        size
    }
}

// ----------------------------------------------
// TopBarStats
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
struct TopBarStats {
    population: u32,
    gold: u32,
}

impl TopBarStats {
    fn new(context: &UiWidgetContext) -> Self {
        Self {
            population: context.world.stats().population.total,
            gold: context.world.stats().treasury.gold_units_total,
        }
    }
}

// ----------------------------------------------
// TopBar
// ----------------------------------------------

struct TopBar {
    current_stats: TopBarStats,
    icon_label_indices: [Option<usize>; TOP_BAR_ICON_COUNT],
    menu: UiMenuRcMut,
}

impl MenuBar for TopBar {
    fn draw(&mut self, context: &mut UiWidgetContext) {
        let stats = TopBarStats::new(context);

        if self.current_stats != stats {
            self.update_stats(stats);
        }

        self.menu.draw(context);
    }
}

impl TopBar {
    fn new(context: &mut UiWidgetContext) -> Rc<Self> {
        let stats = TopBarStats::new(context);

        let mut group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: TOP_BAR_ICON_SPACING,
                center_horizontally: false, // Let content float left.
                stack_vertically: false, // Layout icons side-by-side.
                ..Default::default()
            }
        );

        let mut icon_label_indices = [None; TOP_BAR_ICON_COUNT];

        for (icon_index, icon) in TopBarIcon::iter().enumerate() {
            let icon_tooltip = {
                if icon.with_tooltip() {
                    Some(UiTooltipText::new(
                        context,
                        UiTooltipTextParams {
                            text: icon.to_string(),
                            font_scale: TOOLTIP_FONT_SCALE,
                            background: Some(TOOLTIP_BACKGROUND_SPRITE),
                        }
                    ))
                } else {
                    None
                }
            };

            let icon_sprite = UiSpriteIcon::new(
                context,
                UiSpriteIconParams {
                    sprite: icon.asset_path().to_str().unwrap(),
                    size: icon.size(),
                    margin_top: icon.margin_top(),
                    tooltip: icon_tooltip,
                    clip_to_parent_menu: icon.clip_to_menu(),
                    unclipped_draw_size: Some(icon.size_unclipped()),
                    ..Default::default()
                }
            );

            group.add_widget(icon_sprite);

            if let Some(label_text) = icon.label_for_stats(&stats) {
                let icon_label = UiSizedTextLabel::new(
                    context,
                    UiSizedTextLabelParams {
                        font_scale: TOOLTIP_FONT_SCALE,
                        label: label_text,
                        size: TopBarIcon::max_label_size(context),
                    }
                );

                let label_index = group.add_widget(icon_label);
                icon_label_indices[icon_index] = Some(label_index);
            }

            let is_last = icon_index == (TOP_BAR_ICON_COUNT - 1);
            if !is_last {
                const SEPARATOR_WIDTH: f32 = 20.0;

                let spacing = UiSeparator::new(
                    context,
                    UiSeparatorParams {
                        size: Some(Vec2::new(SEPARATOR_WIDTH, 0.0)),
                        ..Default::default()
                    }
                );

                group.add_widget(spacing);
            }
        }

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("TopBar".into()),
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenterTop,
                widget_spacing: Some(Vec2::new(TOP_BAR_ICON_SPACING, TOP_BAR_ICON_SPACING)),
                background: Some("misc/wide_page_bg.png"),
                ..Default::default()
            }
        );

        menu.add_widget(group);

        Rc::new(Self { current_stats: stats, icon_label_indices, menu })
    }

    fn update_stats(&mut self, stats: TopBarStats) {
        self.current_stats = stats;

        let (_, group) = self.menu
            .find_widget_of_type_mut::<UiWidgetGroup>()
            .unwrap();

        let widgets = group.widgets_mut();

        for (icon_index, label_index) in self.icon_label_indices.iter().enumerate() {
            if let Some(widget_index) = *label_index {
                let icon = TopBarIcon::try_from_primitive(icon_index).unwrap();

                let widget = &mut widgets[widget_index];
                let label = widget.as_any_mut().downcast_mut::<UiSizedTextLabel>().unwrap();

                label.set_label(icon.label_for_stats(&stats).unwrap());
            }
        }
    }
}

// ----------------------------------------------
// LeftBarButtonKind
// ----------------------------------------------

const LEFT_BAR_BUTTON_SIZE: Vec2 = Vec2::new(24.0, 24.0);
const LEFT_BAR_BUTTON_SPACING: f32 = 4.0;

const LEFT_BAR_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED: bool = true;
const LEFT_BAR_BUTTON_STATE_TRANSITION_SECS: Seconds = 0.5;

const LEFT_BAR_BUTTON_COUNT: usize = LeftBarButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum LeftBarButtonKind {
    #[strum(props(Label = "menu_bar/main_menu", Tooltip = "Game"))]
    MainMenu,

    #[strum(props(Label = "menu_bar/save_game", Tooltip = "Load / Save"))]
    SaveGame,

    #[strum(props(Label = "menu_bar/settings"))]
    Settings,
}

impl LeftBarButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = true;
        match self {
            Self::MainMenu => dialog::open(DialogMenuKind::MainMenu, CLOSE_ALL_OTHERS, context),
            Self::SaveGame => dialog::open(DialogMenuKind::LoadOrSaveGame, CLOSE_ALL_OTHERS, context),
            Self::Settings => dialog::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context),
        }
    }
}

impl ButtonDef for LeftBarButtonKind {}

// ----------------------------------------------
// LeftBar
// ----------------------------------------------

struct LeftBar {
    menu: UiMenuRcMut,
}

impl MenuBar for LeftBar {
    fn draw(&mut self, context: &mut UiWidgetContext) {
        self.menu.draw(context);
    }
}

impl LeftBar {
    fn new(context: &mut UiWidgetContext) -> Rc<Self> {
        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("LeftBar".into()),
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignLeft,
                position: UiMenuPosition::Vec2(0.0, 60.0),
                widget_spacing: Some(Vec2::new(LEFT_BAR_BUTTON_SPACING, LEFT_BAR_BUTTON_SPACING)),
                background: Some("misc/tall_page_bg.png"),
                ..Default::default()
            }
        );

        for button_kind in LeftBarButtonKind::iter() {
            let on_button_state_changed = UiSpriteButtonStateChanged::with_closure(
                move |button, context, _| {
                    if button.is_pressed() {
                        button_kind.on_pressed(context);

                        // Pressed state doesn't persist.
                        button.press(false);
                    }
                }
            );

            let button = button_kind.new_sprite_button(
                context,
                LEFT_BAR_BUTTON_SHOW_TOOLTIP_WHEN_PRESSED,
                LEFT_BAR_BUTTON_SIZE,
                LEFT_BAR_BUTTON_STATE_TRANSITION_SECS,
                UiSpriteButtonState::Idle,
                on_button_state_changed
            );

            menu.add_widget(button);
        }

        Rc::new(Self { menu })
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
    fn draw(&mut self, context: &mut UiWidgetContext) {
        let sim_state = SimState::new(context);

        if self.current_sim_state != sim_state {
            self.update_sim_state(context, sim_state);
        }

        self.menu.draw(context);
    }
}

impl SpeedControlsBar {
    fn new(context: &mut UiWidgetContext) -> Rc<Self> {
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
        let (label_text, label_size) = sim_state.label_and_size(context);

        let sim_state_label = UiSizedTextLabel::new(
            context,
            UiSizedTextLabelParams {
                font_scale: TOOLTIP_FONT_SCALE,
                label: label_text,
                size: label_size,
            }
        );

        group.add_widget(sim_state_label);

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some("SpeedControlsBar".into()),
                flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignLeft,
                widget_spacing: Some(Vec2::new(SPEED_CONTROLS_BUTTON_SPACING, SPEED_CONTROLS_BUTTON_SPACING)),
                background: Some("misc/wide_page_bg.png"),
                ..Default::default()
            }
        );

        menu.add_widget(group);

        Rc::new(Self { current_sim_state: sim_state, menu })
    }

    fn update_sim_state(&mut self, context: &UiWidgetContext, sim_state: SimState) {
        self.current_sim_state = sim_state;

        let (_, group) = self.menu
            .find_widget_of_type_mut::<UiWidgetGroup>()
            .unwrap();

        let (_, label) = group
            .find_widget_of_type_mut::<UiSizedTextLabel>()
            .unwrap();

        let (label_text, label_size) = sim_state.label_and_size(context);

        label.set_label(label_text);
        label.set_size(label_size);
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

    fn label_and_size(&self, context: &UiWidgetContext) -> (String, Vec2) {
        let label = self.to_string();

        let mut size = context.calc_text_size(TOOLTIP_FONT_SCALE, &label);
        size += Vec2::new(10.0, 5.0); // explicit padding.

        (label, size)
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

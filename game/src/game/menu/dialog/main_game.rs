use strum::EnumCount;
use strum_macros::{EnumProperty, EnumCount, EnumIter};

use super::*;
use crate::{
    implement_dialog_menu,
    game::{
        GameLoop,
        menu::{ButtonDef, LARGE_HORIZONTAL_SEPARATOR_SPRITE},
    }
};

// ----------------------------------------------
// MainGameButtonKind
// ----------------------------------------------

const MAIN_GAME_BUTTON_COUNT: usize = MainGameButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum MainGameButtonKind {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Load Game"))]
    LoadGame,

    #[strum(props(Label = "Save Game"))]
    SaveGame,

    #[strum(props(Label = "Settings"))]
    Settings,

    #[strum(props(Label = "Quit"))]
    Quit,

    #[strum(props(Label = "Back ->"))]
    Back,
}

impl ButtonDef for MainGameButtonKind {
    fn on_pressed(self, context: &mut UiWidgetContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::NewGame  => super::open(DialogMenuKind::NewGame,  CLOSE_ALL_OTHERS, context),
            Self::LoadGame => super::open(DialogMenuKind::LoadGame, CLOSE_ALL_OTHERS, context),
            Self::SaveGame => super::open(DialogMenuKind::SaveGame, CLOSE_ALL_OTHERS, context),
            Self::Settings => super::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context),
            Self::Quit     => Self::on_quit(context),
            Self::Back     => super::close_current(context),
        }
    }
}

impl MainGameButtonKind {
    fn on_quit(context: &mut UiWidgetContext) -> bool {
        let main_menu = DialogMenusSingleton::get_mut()
            .current_dialog_as::<MainGame>()
            .expect("Expected MainGame dialog to be open!");

        main_menu.open_quit_game_message_box(context)
    }
}

// ----------------------------------------------
// MainGame
// ----------------------------------------------

pub struct MainGame {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { MainGame, ["Game"] }

impl MainGame {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let buttons = make_dialog_button_widgets::<MainGameButtonKind, MAIN_GAME_BUTTON_COUNT>(context);

        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_BUTTON_SPACING,
            Some(buttons)
        );

        menu.set_flags(UiMenuFlags::HideWhenMessageBoxOpen, true);

        Self { menu }
    }

    fn open_quit_game_message_box(&mut self, context: &mut UiWidgetContext) -> bool {
        debug_assert!(self.menu.is_open());

        if self.menu.is_message_box_open() {
            return false;
        }

        let menu_rc = self.menu.clone();

        self.menu.open_message_box(context, |context: &mut UiWidgetContext| {
            let menu_weak_ref = menu_rc.downgrade();
            UiMessageBoxParams {
                label: Some("Quit Game Popup".into()),
                background: Some(DEFAULT_DIALOG_POPUP_BACKGROUND_SPRITE),
                contents: vec![
                    UiWidgetImpl::from(UiMenuHeading::new(
                        context,
                        UiMenuHeadingParams {
                            lines: vec![
                                "Quit Game?".into(),
                                "Any unsaved progress will be lost...".into(),
                            ],
                            font_scale: DEFAULT_DIALOG_POPUP_FONT_SCALE,
                            separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            margin_top: 2.0,
                            ..Default::default()
                        }
                    ))
                ],
                buttons: vec![
                    UiWidgetImpl::from(UiTextButton::new(
                        context,
                        UiTextButtonParams {
                            label: "Quit to Main Menu".into(),
                            size: UiTextButtonSize::Normal,
                            hover: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            enabled: true,
                            on_pressed: UiTextButtonPressed::with_fn(|_, _| GameLoop::get_mut().quit_to_main_menu()),
                            ..Default::default()
                        }
                    )),
                    UiWidgetImpl::from(UiTextButton::new(
                        context,
                        UiTextButtonParams {
                            label: "Exit Game".into(),
                            size: UiTextButtonSize::Normal,
                            hover: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            enabled: true,
                            on_pressed: UiTextButtonPressed::with_fn(|_, _| GameLoop::get_mut().request_quit()),
                            ..Default::default()
                        }
                    )),
                    UiWidgetImpl::from(UiTextButton::new(
                        context,
                        UiTextButtonParams {
                            label: "Cancel".into(),
                            size: UiTextButtonSize::Normal,
                            hover: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                            enabled: true,
                            on_pressed: UiTextButtonPressed::with_closure(
                                move |_, context| {
                                    let mut main_menu = menu_weak_ref.upgrade().unwrap();
                                    main_menu.close_message_box(context);
                                }
                            ),
                            ..Default::default()
                        }
                    )),
                ],
                ..Default::default()
            }
        });

        true
    }
}

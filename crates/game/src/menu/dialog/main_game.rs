use strum::{EnumCount, EnumIter, EnumProperty};

use super::*;
use crate::{GameLoop, menu::ButtonDef};

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
    fn on_pressed(self, context: &mut GameUiContext) -> bool {
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
    fn on_quit(context: &mut GameUiContext) -> bool {
        let main_menu = super::current_as::<MainGame>().expect("Expected MainGame dialog to be open!");

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
    pub fn new(context: &mut GameUiContext) -> Self {
        let buttons = make_dialog_button_widgets::<MainGameButtonKind, MAIN_GAME_BUTTON_COUNT>(context);
        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_BUTTON_SPACING, buttons);

        menu.set_flags(UiMenuFlags::HideWhenMessageBoxOpen, true);

        Self { menu }
    }

    fn open_quit_game_message_box(&mut self, context: &mut GameUiContext) -> bool {
        debug_assert!(self.menu.is_open());

        if self.menu.is_message_box_open() {
            return false;
        }

        let menu_weak_ref = self.menu.downgrade();

        self.menu.open_message_box(context, |context| {
            let lines = ["Quit Game?", "Any unsaved progress will be lost..."];
            make_dialog_popup(context, "Quit Game Popup", lines).with_buttons([
                make_text_button(context, "Quit to Main Menu", |_, _| GameLoop::get_mut().quit_to_main_menu()),
                make_text_button(context, "Exit Game", |_, _| GameLoop::get_mut().quit_game()),
                make_text_button(context, "Cancel", move |_, context| {
                    let mut main_menu = menu_weak_ref.upgrade().unwrap();
                    main_menu.close_message_box(context);
                }),
            ])
        });

        true
    }
}

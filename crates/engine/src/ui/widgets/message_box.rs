use std::any::Any;

use common::Vec2;

use super::{UiMenu, UiMenuFlags, UiMenuInsets, UiMenuRcMut, UiWidget, UiWidgetContext, UiWidgetGroup, UiWidgetImpl};
use crate::{
    file_sys::paths::PathRef,
    ui::{UiFontScale, sound::{UiSoundEvent, UiSoundKey}},
};

// ----------------------------------------------
// UiMessageBox
// ----------------------------------------------

// A modal popup of a menu: its contents, then its buttons side by side. The menu builds it when
// first opened (UiMenu::open_message_box) and keeps it until reset.
#[derive(Default)]
pub struct UiMessageBox {
    pub(super) menu: Option<UiMenuRcMut>,
}

impl UiWidget for UiMessageBox {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        if let Some(menu) = &self.menu
            && menu.is_open()
        {
            // NOTE: Increment the ref count here.
            // draw() may trigger a UiMessageBox::reset, which could drop `self.menu`.
            let mut strong_ref = menu.clone();
            strong_ref.draw(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.menu.as_ref().map_or(Vec2::zero(), |menu| menu.measure(context))
    }

    fn label(&self) -> &str {
        self.menu.as_ref().map_or("", |menu| menu.label())
    }

    fn font_scale(&self) -> UiFontScale {
        self.menu.as_ref().map_or(UiFontScale::default(), |menu| menu.font_scale())
    }
}

impl UiMessageBox {
    const BUTTON_SPACING: Vec2 = Vec2::new(10.0, 10.0);

    // A closed popup centered on the screen, with the style's frame. `label` names it in widget
    // paths. Escape closes it. It plays PopupOpened and PopupClosed.
    pub fn new(label: impl Into<String>) -> Self {
        let menu = UiMenu::new(label)
            .with_flags(UiMenuFlags::AlignCenter | UiMenuFlags::Modal | UiMenuFlags::CloseModalOnEscape)
            .with_sound(UiSoundEvent::Opened, Some(UiSoundKey::PopupOpened))
            .with_sound(UiSoundEvent::Closed, Some(UiSoundKey::PopupClosed));
        Self { menu: Some(menu.into_rc()) }
    }

    // `event` (Opened or Closed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.menu_mut().set_sound(event, role);
        self
    }

    // A sprite stretched over the popup, in place of the style's frame.
    pub fn with_background<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.menu_mut().background = Some(context.load_texture(sprite.into()));
        self
    }

    // The borders of the background sprite: the contents stay inside them.
    pub fn with_content_insets(mut self, insets: UiMenuInsets) -> Self {
        self.menu_mut().content_insets = Some(insets);
        self
    }

    // A widget of the popup's contents, above its buttons.
    pub fn with_content(mut self, widget: impl Into<UiWidgetImpl>) -> Self {
        self.menu_mut().add_widget(widget);
        self
    }

    // Its buttons, side by side under the contents: so give them last.
    pub fn with_buttons<I>(mut self, buttons: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<UiWidgetImpl>,
    {
        let mut group = UiWidgetGroup::new()
            .with_widget_spacing(Self::BUTTON_SPACING)
            .with_center_vertically(true)
            .with_stack_vertically(false);

        for button in buttons {
            group.add_widget(button);
        }

        self.menu_mut().add_widget(group);
        self
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.is_open())
    }

    pub fn open(&mut self, context: &mut dyn UiWidgetContext) {
        if let Some(menu) = &mut self.menu {
            menu.open(context);
        }
    }

    pub fn close(&mut self, context: &mut dyn UiWidgetContext) {
        if let Some(menu) = &mut self.menu {
            menu.close(context);
        }
    }

    pub fn reset(&mut self) {
        self.menu = None;
    }

    fn menu_mut(&mut self) -> &mut UiMenu {
        self.menu.as_mut().expect("UiMessageBox::new builds the menu!")
    }
}

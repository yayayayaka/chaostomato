pub(crate) mod inline {
    use tbot::types::keyboard::inline::{Button, ButtonKind::CallbackData, Markup};

    pub const START_MENU: Markup = &[&[
        Button::new("/25", CallbackData("25")),
        Button::new("/5", CallbackData("5")),
        Button::new("help", CallbackData("help")),
        Button::new("cancel", CallbackData("cancel")),
    ]];
    pub const ASK_TO_CONTINUE: Markup = &[&[
        Button::new("Yes", CallbackData("25")),
        Button::new("No, thanks", CallbackData("cancel")),
    ]];
    pub const JOIN: Markup = &[&[Button::new("Join", CallbackData("join"))]];
    pub const GOT_IT: Markup = &[&[Button::new("Got it!", CallbackData("cancel"))]];
}

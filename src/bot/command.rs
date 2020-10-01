use std::sync::Arc;

use tbot::contexts::fields::{Context, Message};
use tbot::contexts::methods::ChatMethods;
use tbot::contexts::{Command, Text};

use tbot::types::keyboard::inline::Keyboard;

use crate::bot::util;
use crate::markup::inline::START_MENU;
use crate::state::State;

/// Start command
///
/// If the command is a reply to a registered Pomodoro, attempt to start the pomodoro
pub(crate) async fn start(context: Arc<Command<Text>>, state: Arc<State>) {
    // Attempt to start the session if the message is a reply to the bot
    if let Some(message) = &context.reply_to {
        if let Some(user) = context.from() {
            if let Ok(_) = util::start_pomodoro_now(context.bot(), user, message, state).await {
                return;
            }
        } else {
            dbg!("User not found");
        }
    }
    let text = "Choose one of the following:";

    if let Err(call_result) = context
        .send_message(text)
        .reply_markup(Keyboard::new(START_MENU))
        .call()
        .await
    {
        dbg!(call_result);
        return;
    }
}

/// Command to display information on usage
pub(crate) async fn help(context: Arc<Command<Text>>, _state: Arc<State>) {
    util::send_help_text(context.bot(), context.chat.id).await
}

/// Command to create a 25 minute long Pomodoro session
pub(crate) async fn _25(context: Arc<Command<Text>>, state: Arc<State>) {
    let from_user = match context.from.to_owned() {
        Some(user) => user,
        None => {
            dbg!("Could not unwrap User");
            return;
        }
    };
    util::create_pomodoro(
        context.bot(),
        state.clone(),
        context.chat.to_owned(),
        from_user,
    )
    .await;
}

/// Command to create a 5 minute break
pub(crate) async fn _5(context: Arc<Command<Text>>, state: Arc<State>) {
    if let Some(user) = context.from.to_owned() {
        util::_5_minute_break(context.bot(), state, context.chat.to_owned(), user).await;
    } else {
        dbg!("Could not extract user!");
    }
}

/// Join a Session
pub(crate) async fn join(context: Arc<Command<Text>>, state: Arc<State>) {
    match context.from() {
        Some(user) => match state.join_latest_session(context.chat(), user) {
            Ok(message) => {
                state
                    .update_participants_text(context.bot(), &message)
                    .await;
            }
            Err(err) => {
                if let Err(err) = context.send_message_in_reply(&err).call().await {
                    dbg!(err.to_string());
                }
            }
        },
        None => {
            dbg!("Could not determine user");
            return;
        }
    }
}

/// Leave from a subscribed Pomodoro
///
/// This tries to figure out the most recent session the user is subscribed and if successful, unsubscribes the user
/// Otherwise the bot will replay they didn't found a session
pub(crate) async fn leave(context: Arc<Command<Text>>, state: Arc<State>) {
    let user = match context.from() {
        Some(user) => user,
        None => {
            dbg!("Could not determine user");
            return;
        }
    };
    match state
        .leave_latest_session(context.bot(), context.chat(), user)
        .await
    {
        Ok(_msg) => {}
        Err(err) => {
            dbg!(err.to_string());
        }
    }
}

use std::sync::Arc;

use tbot::{
    types::{chat, chat::Kind, Chat, Message, User},
    Bot,
};

use crate::{markup::inline, state::State, time};


/// Register a new Pomodoro
pub(crate) async fn create_pomodoro(bot: &Bot, state: Arc<State>, chat: Chat, from_user: User) {
    let message_content = match chat.kind {
        Kind::Group { .. } | Kind::Supergroup { .. } => {
            let hh_mm = time::future_point_as_hh_mm();
            format!(
                "@{} has created a new Pomodoro!\n\
            Session will start at {} (UTC)\n\n\
            Subscribers:",
                from_user.username.to_owned().unwrap(),
                hh_mm
            )
        }
        Kind::Private { .. } => "Pomodoro session has been started!".to_string(),
        _ => {
            dbg!("Message is not from a group or private chat");
            return;
        }
    };
    let send_message = match chat.kind {
        Kind::Group { .. } | Kind::Supergroup { .. } => bot
            .send_message(*&chat.id, &message_content)
            .reply_markup(inline::JOIN),
        Kind::Private { .. } => bot.send_message(*&chat.id, &message_content),
        _ => {
            dbg!("Message is not from a group or private chat");
            return;
        }
    };
    match send_message.call().await {
        Ok(message) => {
            if let Err(msg) = state
                .new_pomodoro(message.to_owned(), from_user, None, None)
                .await
            {
                dbg!(msg);
                return;
            }
            match message.chat.kind {
                chat::Kind::Supergroup { .. } | chat::Kind::Group { .. } => {
                    state.update_participants_text(bot, &message).await
                }
                _ => {}
            }
        }
        Err(e) => {
            dbg!(e);
            return;
        }
    }
}

/// Start a 5 minute break
pub(crate) async fn _5_minute_break(bot: &Bot, state: Arc<State>, chat: Chat, user: User) {
    let username = match &user.username {
        Some(user) => user,
        _ => &user.first_name,
    };
    let message_content = match chat.kind {
        Kind::Group { .. } | Kind::Supergroup { .. } => {
            format!("@{}, your 5 minute break has begun!", username)
        }
        _ => "Your 5 minute break has begun!".to_string(),
    };
    match bot.send_message(chat.id, &message_content).call().await {
        Ok(message) => {
            if let Err(err) = state.new_break(message, user, None, None) {
                dbg!(err);
            }
        }
        Err(e) => {
            dbg!(e);
        }
    }
}

/// Display information on usage
pub(crate) async fn send_help_text(bot: &Bot, chat_id: chat::Id) {
    let bot_username = match bot.get_me().call().await {
        Ok(me) => format!("@{}", me.user.username.unwrap_or(me.user.first_name)),
        Err(err) => {
            dbg!(err.to_string());
            "".to_string()
        }
    };

    if let Err(err_msg) = bot
        .send_message(
            chat_id,
            &format!(
                "\
{} — Yet another Pomodoro Timer bot for telegram.

Commands:
/25 — Create a new Timer with a duration of 25 minutes.
/5 — Initiate a short 5 minute break
/join — Join a session
/leave — Leave a session
/help — Show this help message.

This bot supports multiplayer mode!
Create a /25 in a group and a button will show up for others \
to join. As soon as the clock hits `minute % 5 == 0`, you will be pinged to start your session.

Made with 🥰🦀 by @yayayayaka
https://github.com/yayayayaka/chaostomato",
                bot_username
            ),
        )
        .reply_markup(inline::GOT_IT)
        .call()
        .await
    {
        dbg!(err_msg.to_string());
    }
}

/// Attempt to start a pomodoro now
pub(crate) async fn start_pomodoro_now(
    bot: &Bot,
    user: &User,
    message: &Message,
    state: Arc<State>,
) -> Result<String, String> {
    match state.start_session_now(bot, user, message).await {
        Ok(ok) => Ok(ok),
        Err(err) => Err(err),
    }
}

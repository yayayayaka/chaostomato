use core::{
    option::Option::Some,
    result::Result::{Err, Ok},
    time::Duration,
};
use std::sync::Arc;

use futures_util::stream::poll_fn;
use tbot::{types::chat, Bot};
use tokio::{join, stream::StreamExt, time::delay_for};

use super::{session::Session, State};

/// Periodically poll for expired entries from the DelayQueue
pub(crate) async fn poll_for_expired_entries(bot: Bot, state: Arc<State>) {
    // There might be a better way to poll new expirations, but this should be fine for now...
    while let item = poll_fn(|cx| state.expirations.lock().unwrap().poll_expired(cx))
        .next()
        .await
    {
        if let Some(Ok(result)) = item {
            let cache_key = result.into_inner();
            let entry;
            {
                entry = state.entries.lock().unwrap().remove(&cache_key)
            }
            if let Some((session, _key)) = entry {
                if session.is_waiting() {
                    start_pomodoro(&bot, state.clone(), session).await;
                } else if session.is_running() {
                    end_pomodoro(&bot, state.clone(), session).await;
                } else if session.is_awaiting_break() {
                    start_break(state.clone(), session);
                } else if session.is_taking_a_break() {
                    end_break(&bot, session).await;
                }
            }
        } else {
            delay_for(Duration::from_secs(1)).await;
        }
    }
}

fn start_break(state: Arc<State>, session: Session) {
    state.start_break(session);
}

/// Start a new pomodoro session
async fn start_pomodoro(bot: &Bot, state: Arc<State>, mut pomodoro: Session) {
    match pomodoro.message().chat.kind {
        chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => {
            pomodoro.notify_participants_on_start(bot).await;
            state.start_session(pomodoro);
        }
        chat::Kind::Private { .. } => {
            state.start_session(pomodoro);
        }
        _ => {
            dbg!("/start called outside of a chat");
        }
    }
}

/// End a running pomodoro session.
async fn end_pomodoro(bot: &Bot, state: Arc<State>, mut pomodoro: Session) {
    if let Err(err_msg) = pomodoro.notify_participants_on_end(&bot).await {
        dbg!(err_msg.to_string());
    }

    state.start_break(pomodoro);
}

async fn end_break(bot: &Bot, pomodoro: Session) {
    match pomodoro.message.chat.kind {
        chat::Kind::Private { .. } => {
            join!(
                async {
                    if let Err(err_msg) = bot
                        .delete_message(pomodoro.chat().id, pomodoro.message().id)
                        .call()
                        .await
                    {
                        dbg!(err_msg.to_string());
                    }
                },
                async {
                    if let Err(err_msg) = pomodoro.notify_participants_on_break_end(&bot).await {
                        dbg!(err_msg.to_string());
                    }
                },
            );
        }
        _ => {
            if let Err(err_msg) = pomodoro.notify_participants_on_break_end(&bot).await {
                dbg!(err_msg.to_string());
            }
        }
    }
}

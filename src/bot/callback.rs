use std::{borrow::Borrow, sync::Arc};

use tbot::contexts::{
    fields::{Callback, Context},
    DataCallback,
};
use tokio::join;

use super::util;
use crate::state::State;
use tbot::contexts::methods::Callback as OtherCallback;

/// Data callback handler
pub(crate) async fn data_callback(context: Arc<DataCallback>, state: Arc<State>) {
    // I don't get it working via data_callback_if yet...
    match context.data.as_str() {
        "25" => _25_pressed(context, state).await,
        "5" => _5_pressed(context, state).await,
        "help" => help_pressed(context).await,
        "cancel" => cancel_button_pressed(context).await,
        "join" => join_pressed(context, state).await,
        "start now" => start_now_pressed(context, state).await,
        unhandled => {
            dbg!(format!("Received unhandled callback: {}", unhandled));
        }
    }
}

async fn start_now_pressed(context: Arc<DataCallback>, state: Arc<State>) {
    if let Some(message) = context.origin.to_owned().message() {
        match state
            .start_session_now(context.bot(), context.from(), &message)
            .await
        {
            Ok(msg) => {
                context.notify(&*msg).call().await.unwrap();
            }
            Err(msg) => {
                context.notify(&*msg).call().await.unwrap();
            }
        }
    } else {
        dbg!("Context is not from a Message.");
    }
}

async fn _25_pressed(context: Arc<DataCallback>, state: Arc<State>) {
    join!(delete_message(context.clone()), async {
        if let Some(message) = context.origin.to_owned().message() {
            util::create_pomodoro(
                context.bot(),
                state,
                message.chat.to_owned(),
                context.from.to_owned(),
            )
            .await;
        } else {
            dbg!("Context is not from a Message.");
        }
    });
}

async fn _5_pressed(context: Arc<DataCallback>, state: Arc<State>) {
    join!(delete_message(context.clone()), async {
        if let Some(message) = context.origin.to_owned().message() {
            util::_5_minute_break(context.bot(), state, message.chat, context.from.to_owned()).await
        }
    });
}

async fn help_pressed(context: Arc<DataCallback>) {
    join!(delete_message(context.clone()), async {
        if context.origin.borrow().is_message() {
            util::send_help_text(
                context.bot(),
                context.origin.to_owned().expect_message().chat.id,
            )
            .await;
        } else {
            dbg!("Not a Message");
        }
    },);
}

/// Delete the menu
async fn cancel_button_pressed(context: Arc<DataCallback>) {
    delete_message(context).await
}

async fn join_pressed(context: Arc<DataCallback>, state: Arc<State>) {
    if let Some(message) = context.origin.to_owned().message() {
        match state
            .add_participant(&*context.bot, &message, context.from.to_owned())
            .await
        {
            Ok(msg) => {
                // How do I merge this into one statement?
                context.notify(msg).call().await.unwrap_or_else(|msg| {
                    dbg!(msg);
                });
            }
            Err(msg) => {
                dbg!(msg);
            }
        }
    } else {
        dbg!("Context is not a message");
    }
}

/// Delete the Message associated with the DataCallback
async fn delete_message(context: Arc<DataCallback>) {
    match context.origin.to_owned().message() {
        Some(message) => {
            if let Err(message) = context
                .bot()
                .delete_message(message.chat.id, message.id)
                .call()
                .await
            {
                dbg!(message.to_string());
            }
        }
        None => {
            dbg!("Could not extract message.");
        }
    }
}

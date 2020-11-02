use core::time::Duration;
use std::collections::HashSet;

use tbot::{errors::MethodCall, types, types::chat, Bot};
use tokio::{join, time::Instant};

use crate::{markup, time};

use super::session_state::SessionState;


/// A struct that holds a Session
///
/// A Session is distinguished by it's session state. A session can represent either:
/// - a registered Pomodoro session registered to start
/// - a running Pomodoro session
/// - a registered break scheduled to start
/// - a running break
///
/// Sessions are identified by a a combination of chat id + message id.
/// There can be at most one registered Pomodoro per message.
#[derive(Debug, Clone)]
pub struct Session {
    /// Current state of the Pomodoro. See 'SessionState' for more info.
    pub(super) state: SessionState,

    /// The associated message that acts as an identifier.
    /// That message is being used to notify
    pub(super) message: types::Message,

    /// The user who creates the session
    pub(super) creator: types::User,

    /// A HashSet of participants who joined the session.
    /// (The creator of the session is always included).
    pub(super) participants: HashSet<types::User>,

    /// A timestamp of the creation time
    pub(super) creation_time: Instant,

    /// The timestamp when the session shall be started.
    ///
    /// The default value depends on the session type and the kind of chat where the session was
    /// registered.
    ///
    /// If the session is a break, then the default start time is "now".
    /// If the session is a Pomodoro, then the start time is:
    /// - "now" for Pomodoros created in private chats,
    /// - the next `minute % 5 == 0` of an hour for Pomodoros created in Groups or SuperGroups.
    pub(super) start_time: Instant,

    /// Duration of the session
    ///
    /// Defaults to
    /// - 25 minutes for Pomodoros,
    /// -  5 minutes for breaks
    /// unless otherwise specified.
    pub(super) duration: Duration,
}

impl Session {
    /// Create a new Pomodoro Session
    ///
    /// Depending on `chat_kind`, the Pomodoro is scheduled to start either immediately or at the
    /// next `minute % 5 == 0` of the current hour.
    pub(super) fn new_pomodoro(
        message: types::Message,
        creator: types::User,
        start_time: Option<Instant>,
        duration: Option<Duration>,
    ) -> Result<Session, String> {
        let mut participants = HashSet::new();
        participants.insert(creator.to_owned());

        let creation_time = Instant::now();
        let duration = duration.unwrap_or(Duration::from_secs(60 * 25));

        match message.chat.kind {
            chat::Kind::Private { .. } => Ok(Session {
                message,
                creator,
                participants,
                creation_time,
                start_time: start_time.unwrap_or(Instant::now()),
                duration,
                state: SessionState::PomodoroWaiting,
            }),
            chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => Ok(Session {
                message,
                creator,
                participants,
                creation_time,
                start_time: start_time.unwrap_or_else(|| time::instant_at_minute()),
                duration,
                state: SessionState::PomodoroWaiting,
            }),
            _ => {
                let err_msg =
                    "Chat kind is neither a group nor a supergroup nor a private chat".to_string();
                dbg!(&err_msg);
                Err(err_msg)
            }
        }
    }

    /// Create a new Break Session
    pub(super) fn new_break(
        message: types::Message,
        creator: types::User,
        start_time: Option<Instant>,
        duration: Option<Duration>,
    ) -> Result<Session, String> {
        let mut participants = HashSet::new();
        participants.insert(creator.to_owned());

        let creation_time = Instant::now();
        let duration = duration.unwrap_or_else(|| Duration::from_secs(60 * 5));

        Ok(Session {
            message,
            creator,
            participants,
            creation_time,
            start_time: start_time.unwrap_or(creation_time),
            duration,
            state: SessionState::BreakWaiting,
        })
    }

    /// Convert a pomodoro session to a break session
    pub(crate) fn convert_to_break(&mut self) {
        self.duration = Duration::from_secs(60 * 5);
        self.state = SessionState::BreakRunning;
    }

    /// Return true if the session is a running Pomodoro session.
    pub(super) fn is_running(&self) -> bool {
        self.state.eq(&SessionState::PomodoroRunning)
    }

    /// Return true if the session is a running Break session.
    pub(super) fn is_taking_a_break(&self) -> bool {
        self.state.eq(&SessionState::BreakRunning)
    }

    /// Return true if the session is a Pomodoro session that is scheduled to start.
    pub(super) fn is_waiting(&self) -> bool {
        self.state.eq(&SessionState::PomodoroWaiting)
    }

    /// Return true if the session is a Break session that is scheduled to start.
    pub(super) fn is_awaiting_break(&self) -> bool {
        self.state.eq(&SessionState::BreakWaiting)
    }

    /// Delete the previous message and replace it with the ping to all participants
    pub(super) async fn notify_participants_on_start(&mut self, bot: &Bot) {
        let text = format!(
            "{}\n\n\
        \
         Session has started!",
            self.string_of_subscribed_usernames()
        );
        let message_id = self.message.id;
        let chat_id = self.message.chat.id;
        let (delete_message_result, send_message_result) = join!(
            bot.delete_message(chat_id, message_id).call(),
            bot.send_message(chat_id, &text).call()
        );
        if let Err(err) = delete_message_result {
            dbg!(err);
        }

        match send_message_result {
            Ok(message) => {
                self.message = message;
            }
            Err(err) => {
                dbg!(err.to_string());
                return;
            }
        }
    }

    pub(super) async fn notify_participants_on_end(
        &mut self,
        bot: &Bot,
    ) -> Result<types::Message, MethodCall> {
        let text = format!(
            "{}\n\n\
            Session is over! Now take a short, 5 minute break",
            self.string_of_subscribed_usernames()
        );

        match self.message.chat.kind {
            chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => {
                match bot.send_message(self.message.chat.id, &text).call().await {
                    Ok(message) => {
                        self.message = message.to_owned();
                        Ok(message)
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                let (delete_message_result, send_message_result) = join!(
                    bot.delete_message(self.message.chat.id, self.message.id)
                        .call(),
                    bot.send_message(self.message.chat.id, &text).call(),
                );
                if let Err(err) = delete_message_result {
                    dbg!(err.to_string());
                }
                match send_message_result {
                    Ok(message) => {
                        self.message = message.to_owned();
                        Ok(message)
                    }
                    Err(err) => Err(err),
                }
            }
        }
    }

    pub(super) async fn notify_participants_on_break_end(
        &self,
        bot: &Bot,
    ) -> Result<types::Message, MethodCall> {
        let msg = match self.message.chat.kind {
            chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => format!(
                "{}\n\n\
                Break is over!",
                self.string_of_subscribed_usernames()
            ),
            _ => format!("Break is over! Do you want to continue?"),
        };

        match self.message.chat.kind {
            chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => {
                bot.send_message(self.message.chat.id, &msg).call().await
            }
            _ => {
                let (delete_message_result, send_message_result) = join!(
                    bot.delete_message(self.message.chat.id, self.message.id)
                        .call(),
                    bot.send_message(self.message.chat.id, &msg)
                        .reply_markup(markup::inline::ASK_TO_CONTINUE)
                        .call()
                );
                delete_message_result?;
                send_message_result
            }
        }
    }

    /// Return a String of all subscribed usernames
    pub(super) fn string_of_subscribed_usernames(&self) -> String {
        let mut subscribed_users = String::new();
        for user in self.participants.iter() {
            subscribed_users = format!(
                "{} @{}",
                subscribed_users,
                user.username.as_ref().unwrap_or(&user.first_name)
            );
        }
        subscribed_users
    }
}

/// Getters
impl Session {
    // TODO Is it possible to return a reference?
    pub(super) fn chat(&self) -> chat::Chat {
        self.message.chat.to_owned()
    }

    // TODO Is it possible to return a reference?
    pub(super) fn message(&self) -> types::Message {
        self.message.to_owned()
    }
}

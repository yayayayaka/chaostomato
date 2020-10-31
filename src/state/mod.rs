use std::collections::HashMap;
use std::sync::Mutex;

use core::time::Duration;
use tbot::{
    types,
    types::{chat, keyboard::inline, message, user},
    Bot,
};
use tokio::{
    join,
    time::{delay_queue, DelayQueue, Instant},
};

use self::{session::Session, session_state::SessionState};

use crate::markup::inline::JOIN;

pub(crate) mod periodic;
mod session;
mod session_state;

/// The bot's state.
#[derive(Default, Debug)]
pub(crate) struct State {
    /// A queue that holds information about which item is going to expire next.
    pub(self) expirations: Mutex<DelayQueue<CacheKey>>,
    /// A HashMap of saved entries with with information about when the entry shall be yielded back.
    pub(self) entries: Mutex<HashMap<CacheKey, (Session, delay_queue::Key)>>,
}

impl State {
    /// Create a new Pomodoro session and add it to the DelayQueue.
    ///
    /// It it possible to override the default start time and duration by passing `Some(Instant)`
    /// to `start_time` and `Some(Duration)` to `duration`.
    /// The functionality to create sessions with custom durations or start times has not been
    /// implemented on the bot yet.
    pub(crate) async fn new_pomodoro(
        &self,
        message: types::Message,
        creator: types::User,
        start_time: Option<Instant>,
        duration: Option<Duration>,
    ) -> Result<(), String> {
        let cache_key = CacheKey::new(message.chat.id, message.id);
        match self.session_exists(&cache_key) {
            Ok(..) => {
                let err_msg = format!(
                    "Message {}  in chat {} is already present in state",
                    &message.id, &message.chat.id
                );
                dbg!(&err_msg);
                Err(err_msg)
            }
            Err(_) => {
                let pomodoro = Session::new_pomodoro(message, creator, start_time, duration)?;
                self.add_session_to_queue(pomodoro);
                Ok(())
            }
        }
    }

    /// Create a new Break session and add it to the DelayQueue.
    ///
    /// It it possible to override the default start time and duration by passing `Some(Instant)`
    /// to `start_time` and `Some(Duration)` to `duration`.
    /// The functionality to create breaks with custom durations or start times has not been
    /// implemented on the bot yet.
    pub(crate) fn new_break(
        &self,
        message: types::Message,
        creator: types::User,
        start_time: Option<Instant>,
        duration: Option<Duration>,
    ) -> Result<(), String> {
        let cache_key = CacheKey::new(message.chat.id, message.id);
        match self.session_exists(&cache_key) {
            Ok(..) => {
                // Pomodoro is present in state
                let err_msg = format!(
                    "Message {}  in chat {} is already present in state",
                    &message.id, &message.chat.id
                );
                dbg!(&err_msg);
                Err(err_msg)
            }
            Err(_) => {
                // Not present in state
                let pomodoro = Session::new_break(message, creator, start_time, duration)?;
                self.add_session_to_queue(pomodoro);
                Ok(())
            }
        }
    }

    /// Attempt to start a Pomodoro now
    ///
    /// By design, only the creator of the session is permitted to start the session prematurely.
    pub(crate) async fn start_session_now(
        &self,
        bot: &Bot,
        user: &types::User,
        message: &types::Message,
    ) -> Result<String, String> {
        let cache_key = CacheKey::new(message.chat.id, message.id);
        self.session_exists(&cache_key)?;
        if let Err(_) = self.is_owner(&cache_key, &user.id) {
            return Err("Only the creator is allowed to start the session".to_string());
        }

        let result: Option<(Session, delay_queue::Key)>;
        {
            let mut entries = self.entries.lock().unwrap();
            result = entries.remove(&cache_key);
        }
        if let Some((mut pomodoro, key)) = result {
            pomodoro.state = SessionState::PomodoroRunning;
            pomodoro.notify_participants_on_start(bot).await;

            let mut expirations = self.expirations.lock().unwrap();
            expirations.remove(&key);
            let mut entries = self.entries.lock().unwrap();
            let key = expirations.insert(cache_key.to_owned(), pomodoro.duration);
            entries.insert(cache_key, (pomodoro, key));
        }
        Ok("Let's go!".to_string())
    }

    /// Start the session by updating the session state and putting it back into the DelayQueue.
    pub(crate) fn start_session(&self, mut pomodoro: Session) {
        pomodoro.state = SessionState::PomodoroRunning;
        let cache_key = CacheKey::new(pomodoro.message.chat.id, pomodoro.message.id);

        let delay_key = self
            .expirations
            .lock()
            .unwrap()
            .insert(cache_key.clone(), pomodoro.duration);

        self.entries
            .lock()
            .unwrap()
            .insert(cache_key, (pomodoro, delay_key));
    }

    /// Attempt to add a user to the latest registered chat
    pub(crate) fn join_latest_session(
        &self,
        chat: &types::Chat,
        user: &types::User,
    ) -> Result<types::Message, String> {
        match self.newest_session_in_chat(chat) {
            Some(cache_key) => match self.entries.lock() {
                // we have to go deeper!!
                Ok(mut entries) => match entries.get_mut(&cache_key) {
                    Some((session, _key)) => {
                        if session.participants.insert(user.to_owned()) {
                            // the dream is collapsing
                            Ok(session.message.to_owned())
                        } else {
                            Err(format!(
                                "@{} is already a participant",
                                user.username.as_ref().unwrap_or(&user.first_name)
                            ))
                        }
                    }
                    None => {
                        let err = "Session not found in State".to_string();
                        dbg!(&err);
                        Err(err)
                    }
                },
                Err(err) => {
                    dbg!(err.to_string());
                    Err(err.to_string())
                }
            },
            None => Err("This chat does not have any registered sessions yet.\n\n\
            Hint: Use /25 to create a new session."
                .to_string()),
        }
    }

    /// Attempt to remove a user from a Pomodoro based on chat id
    pub(crate) async fn leave_latest_session(
        &self,
        bot: &Bot,
        chat: &types::Chat,
        user: &types::User,
    ) -> Result<String, String> {
        let mut sessions = self.sessions_in_chat(chat);
        sessions.sort_by(|elem_a, elem_b| elem_a.message.id.0.cmp(&elem_b.message.id.0));
        sessions.reverse();
        for entry in sessions {
            if entry.participants.contains(&user) {
                return match self
                    .remove_participant(
                        bot,
                        &CacheKey::new(entry.chat().id, entry.message.id),
                        user,
                    )
                    .await
                {
                    Ok(message) => {
                        self.update_participants_text(bot, &entry.message).await;
                        Ok(message)
                    }
                    Err(err) => Err(err),
                };
            }
        }

        Ok("You are not subscribed to any sessions.".to_string())
    }

    /// Put the Pomodoro back to queue for another 5 minutes
    pub(crate) fn start_break(&self, mut pomodoro: Session) {
        pomodoro.convert_to_break();
        let cache_key = CacheKey::new(pomodoro.message.chat.id, pomodoro.message.id);

        let delay_key = self
            .expirations
            .lock()
            .unwrap()
            .insert(cache_key.clone(), pomodoro.duration);

        self.entries
            .lock()
            .unwrap()
            .insert(cache_key, (pomodoro, delay_key));
    }
}

/// Methods for handling participants
impl State {
    pub(crate) async fn update_participants_text(&self, bot: &Bot, message: &types::Message) {
        let cache_key = CacheKey::new(message.chat.id, message.id);
        let (message, participants) = match self.entries.lock() {
            Ok(entries) => match entries.get(&cache_key) {
                Some((pomodoro, _key)) => (
                    pomodoro.message.to_owned(),
                    pomodoro.participants.to_owned(),
                ),
                None => {
                    dbg!(format!(
                        "Message id {} in chat {} not found!",
                        cache_key.message_id, cache_key.chat_id
                    ));
                    return;
                }
            },
            Err(err) => {
                dbg!(err.to_string());
                return;
            }
        };

        let text = match message.kind.to_owned().text() {
            Some(text) => text,
            _ => {
                dbg!("Message is not a Text");
                return;
            }
        };

        let mut msg: String = text.value;
        let mut split: Vec<&str> = msg.split("Subscribers:").collect();
        if split.len() > 1 {
            split.pop();
        }
        split.push("Subscribers:\n");
        //let subscribed = pomodoro.string_of_subscribed_usernames(bot).await;
        let mut subscribed_users = String::new();
        for user in participants.iter() {
            subscribed_users = format!(
                "{} @{} ",
                subscribed_users,
                user.username.as_ref().unwrap_or(&user.first_name)
            );
        }

        split.push(subscribed_users.trim());
        msg = split.iter().map(|s| s.to_string()).collect();

        let edit_message = match message.chat.kind {
            chat::Kind::Group { .. } | chat::Kind::Supergroup { .. } => bot
                .edit_message_text(message.chat.id, message.id, &msg)
                .reply_markup(inline::Keyboard::new(JOIN)),
            _ => bot.edit_message_text(message.chat.id, message.id, &msg),
        };

        if let Err(err_msg) = edit_message.call().await {
            dbg!(err_msg.to_string());
        }
    }

    /// Add a participant to the session
    pub(crate) async fn add_participant(
        &self,
        bot: &Bot,
        message: &types::Message,
        user: types::User,
    ) -> Result<&'static str, String> {
        let cache_key = CacheKey::new(message.chat.id, message.id);
        if let Err(msg) = self.session_exists(&cache_key) {
            dbg!(&msg);
            return Ok("Pomodoro not found!");
        }

        match self.entries.lock() {
            Ok(mut entries) => {
                if let Some((pomodoro, _key)) = entries.get_mut(&cache_key) {
                    if !pomodoro.participants.insert(user) {
                        return Ok("You are already subscribed!");
                    }
                }
            }
            Err(err) => {
                dbg!(&err.to_string());
                return Err(err.to_string());
            }
        }

        self.update_participants_text(bot, message).await;
        Ok("Yay!")
    }
}

/// Private methods
impl State {
    /// Checks whether a pomodoro exists in chat
    fn session_exists(&self, cache_key: &CacheKey) -> Result<(), String> {
        match self.entries.lock() {
            Ok(entries) => {
                if !entries.contains_key(cache_key) {
                    let err_msg = format!(
                        "A Pomodoro in chat {} with id {} does not exist!",
                        cache_key.chat_id.to_string(),
                        cache_key.message_id.to_string()
                    );
                    return Err(err_msg);
                }
            }
            Err(err) => {
                dbg!(err.to_string());
                return Err(err.to_string());
            }
        }
        Ok(())
    }

    /// Checks whether the specified user is the owner of the session
    fn is_owner(&self, cache_key: &CacheKey, user_id: &user::Id) -> Result<(), String> {
        self.session_exists(cache_key)?;

        match self.entries.lock() {
            Ok(entries) => {
                if let Some((pomodoro, _key)) = entries.get(cache_key) {
                    if pomodoro.creator.id.ne(user_id) {
                        let err_msg = format!(
                            "User id {} is not the owner of Pomodoro {} in chat {}",
                            user_id.to_string(),
                            cache_key.message_id.to_string(),
                            cache_key.chat_id.to_string()
                        );
                        dbg!(&err_msg);
                        return Err(err_msg);
                    }
                }
            }
            Err(err) => {
                dbg!(err.to_string());
                return Err(err.to_string());
            }
        }
        Ok(())
    }

    /// Return a Vec of Sessions for a given chat
    fn sessions_in_chat(&self, chat: &chat::Chat) -> Vec<Session> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(_cache_key, (session, _key))| {
                if session.message.chat.id.eq(&chat.id) {
                    Some(session.to_owned())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Return the newest session in a chat that has not been started yet.
    fn newest_session_in_chat(&self, chat: &chat::Chat) -> Option<CacheKey> {
        let mut sessions = self.sessions_in_chat(chat);
        sessions.sort_by(|elem_a, elem_b| elem_a.message.id.0.cmp(&elem_b.message.id.0));
        sessions = sessions
            .iter()
            .filter_map(|elem| {
                if elem.is_waiting() {
                    Some(elem.to_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<Session>>();
        match sessions.pop() {
            Some(session) => Some(CacheKey::new(session.message.chat.id, session.message.id)),
            _ => None,
        }
    }

    /// Add a Session to the DelayQueue
    fn add_session_to_queue(&self, pomodoro: Session) {
        let cache_key = CacheKey::new(pomodoro.message.chat.id, pomodoro.message.id);
        let delay_key;
        {
            match self.expirations.lock() {
                Ok(mut expirations) => {
                    delay_key = expirations.insert_at(cache_key.clone(), pomodoro.start_time);
                }
                Err(err) => {
                    dbg!(err.to_string());
                    return;
                }
            }
        }
        {
            match self.entries.lock() {
                Ok(mut entries) => {
                    entries.insert(cache_key, (pomodoro, delay_key));
                }
                Err(err) => {
                    dbg!(err.to_string());
                    return;
                }
            }
        }
    }

    /// Remove a session from the DelayQueue
    fn remove_session_from_queue(&self, cache_key: &CacheKey) -> Result<(), String> {
        self.session_exists(&cache_key)?;

        return match self.entries.lock() {
            Ok(mut entries) => {
                if let Some((_, delay_key)) = entries.remove(&cache_key) {
                    return match self.expirations.lock() {
                        Ok(mut expirations) => {
                            expirations.remove(&delay_key);
                            Ok(())
                        }
                        Err(err) => {
                            dbg!(err.to_string());
                            Err(err.to_string())
                        }
                    };
                } else {
                    let err_msg = "Unexpected error".to_string();
                    dbg!(&err_msg);
                    Err(err_msg)
                }
            }
            Err(err) => {
                dbg!(err.to_string());
                Err(err.to_string())
            }
        };
    }

    /// Remove a participant from a session.
    async fn remove_participant(
        &self,
        bot: &Bot,
        cache_key: &CacheKey,
        user: &types::User,
    ) -> Result<String, String> {
        self.session_exists(cache_key)?;
        let mut session_is_empty = false; // work around awaits within a MutexGuard

        let return_val = match self.entries.lock() {
            Ok(mut entries) => {
                if let Some((pomodoro, _key)) = entries.get_mut(cache_key) {
                    pomodoro.participants.retain(|uid| uid.id.ne(&user.id));
                    if pomodoro.creator.eq(&user) {
                        // make someone else the owner
                        match pomodoro.participants.iter().take(1).next() {
                            Some(user) => {
                                pomodoro.creator = user.to_owned();
                            }
                            None => {
                                session_is_empty = true;
                            }
                        }
                    }
                    Ok(format!(
                        "@{} left the session.",
                        user.username.as_ref().unwrap_or(&user.first_name)
                    ))
                } else {
                    let err_msg = format!(
                        "Failed to delete user {} (@{})!",
                        &user.id,
                        user.username.as_ref().unwrap_or(&user.first_name)
                    );
                    dbg!(&err_msg);
                    Err(err_msg)
                }
            }
            Err(err) => {
                dbg!(err.to_string());
                Err(err.to_string())
            }
        };
        if session_is_empty {
            let (delete_message_result, remove_session_result) = join!(
                bot.delete_message(cache_key.chat_id, cache_key.message_id)
                    .call(),
                async { self.remove_session_from_queue(cache_key) }
            );
            if let Err(err) = delete_message_result {
                dbg!(err.to_string());
            }

            if let Err(err) = remove_session_result {
                dbg!(err);
            }
        }

        return return_val;
    }
}

/// A custom identifier of `chat::Id` and `message::Id` that acts as a key for the HashMap and
/// DelayQueue.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct CacheKey {
    pub(self) chat_id: chat::Id,
    pub(self) message_id: message::Id,
}

impl CacheKey {
    /// Return a new CacheKey
    pub(self) fn new(chat_id: chat::Id, message_id: message::Id) -> CacheKey {
        CacheKey {
            chat_id,
            message_id,
        }
    }
}

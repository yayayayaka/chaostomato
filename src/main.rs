use tbot::errors::MethodCall;

use bot::{callback, command};
use state::State;

use state::periodic;

mod bot;
pub(crate) mod markup;
mod state;
mod time;

#[tokio::main]
async fn main() -> Result<(), MethodCall> {
    let bot = tbot::from_env!("BOT_TOKEN");
    let mut event_loop = bot.clone().stateful_event_loop(State::default());

    // Fetch the bot's username
    if let Err(msg) = event_loop.fetch_username().await {
        dbg!(msg);
    }

    // Register bot commands
    event_loop.start(command::start);
    event_loop.help(command::help);
    event_loop.command("25", command::_25);
    event_loop.command("5", command::_5);
    event_loop.command("join", command::join);
    event_loop.command("leave", command::leave);
    event_loop.data_callback(callback::data_callback);

    // The loop to check for expired sessions that need to be handled
    tokio::spawn(periodic::poll_for_expired_entries(
        bot,
        event_loop.get_state(),
    ));

    event_loop.polling().start().await.unwrap();
    Ok(())
}

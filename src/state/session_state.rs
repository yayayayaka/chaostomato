/// An enumeration representing the state of a session.
#[derive(Eq, PartialEq, Debug, Clone)]
pub(super) enum SessionState {
    /// A Pomodoro waiting to be started
    PomodoroWaiting,
    /// A running Pomodoro
    PomodoroRunning,
    /// A break waiting to be started
    BreakWaiting,
    /// A running break
    BreakRunning,
}

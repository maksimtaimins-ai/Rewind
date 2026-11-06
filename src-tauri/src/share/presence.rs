//! Discord Rich Presence: "Playing <game> with Rewind" while a game runs.
//! Off by default. While off nothing connects and nothing is kept.

use std::time::{Duration, Instant};

use discord_rich_presence::activity::{Activity, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

/// Application id of Rewind's Discord application (Discord Developer Portal).
/// Presence stays off until a real one is set here.
pub const APP_ID: &str = "";

/// How long to wait before asking a Discord that isn't running again.
const RETRY: Duration = Duration::from_secs(20);

/// The line to show for `game`, or `None` when nothing should be shown.
pub fn line(enabled: bool, game: Option<&str>) -> Option<String> {
    if !enabled {
        return None;
    }
    game.filter(|g| !g.trim().is_empty()).map(|g| format!("Playing {g}"))
}

#[derive(Default)]
pub struct Presence {
    client: Option<DiscordIpcClient>,
    shown: Option<String>,
    since: i64,
    retry_at: Option<Instant>,
}

impl Presence {
    /// Bring Discord in line with `want`. Cheap when nothing changed, and a
    /// no-op (no socket, no thread) while it is off and was never on.
    pub fn update(&mut self, want: Option<String>) {
        if want == self.shown && (self.client.is_some() || want.is_none()) {
            return;
        }
        let Some(text) = want else {
            self.stop();
            return;
        };
        if APP_ID.is_empty() {
            return;
        }
        if self.client.is_none() {
            if self.retry_at.is_some_and(|t| Instant::now() < t) {
                return;
            }
            let mut c = DiscordIpcClient::new(APP_ID);
            if c.connect().is_err() {
                // Discord isn't running (or isn't listening yet).
                self.retry_at = Some(Instant::now() + RETRY);
                return;
            }
            self.client = Some(c);
            self.retry_at = None;
        }
        if self.shown.is_none() {
            self.since = chrono::Utc::now().timestamp_millis();
        }
        let activity = Activity::new().details(text.as_str()).state("with Rewind").timestamps(Timestamps::new().start(self.since));
        let ok = self.client.as_mut().is_some_and(|c| c.set_activity(activity).is_ok());
        if ok {
            self.shown = Some(text);
        } else {
            // Discord went away; reconnect on the next tick.
            self.client = None;
            self.shown = None;
            self.retry_at = Some(Instant::now() + RETRY);
        }
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.client.take() {
            let _ = c.clear_activity();
            let _ = c.close();
        }
        self.shown = None;
        self.retry_at = None;
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_only_when_on_and_in_a_game() {
        assert_eq!(line(false, Some("CS2")), None);
        assert_eq!(line(true, None), None);
        assert_eq!(line(true, Some("  ")), None);
        assert_eq!(line(true, Some("Counter-Strike 2")).as_deref(), Some("Playing Counter-Strike 2"));
    }

    #[test]
    fn off_never_connects() {
        let mut p = Presence::default();
        p.update(None);
        assert!(p.client.is_none() && p.retry_at.is_none());
    }
}

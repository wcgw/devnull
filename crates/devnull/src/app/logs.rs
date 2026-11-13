//! The log buffers: their names, and polling chronological logs for new commits.

use std::time::Duration;

use devnull_api::Log;

use super::{Dn, Effect, Effects, Kit, Msg, Request, fetch};

/// How often chronological logs are checked for new commits while the terminal has focus, and without it.
const POLL_FOCUSED: Duration = Duration::from_secs(15);
const POLL_UNFOCUSED: Duration = Duration::from_secs(30);

/// A log's buffer name: `log/following` and so on.
pub(super) fn buffer_name(log: Log) -> String {
    format!("log/{}", log.name())
}

/// The logs there are.
const LOGS: [Log; 3] = [Log::Following, Log::Latest, Log::Trending];

/// The log a buffer name names.
pub(super) fn named(name: &str) -> Option<Log> {
    LOGS.into_iter().find(|log| buffer_name(*log) == name)
}

/// The buffer names of the logs.
pub(super) fn names() -> impl Iterator<Item = String> {
    LOGS.into_iter().map(buffer_name)
}

impl Dn {
    /// Starts polling chronological logs for new commits, once.
    pub(super) fn start_polling(&mut self, kit: &Kit) -> Effects {
        if self.polling {
            return Vec::new();
        }
        self.polling = true;
        vec![self.next_poll(kit)]
    }

    /// The wake-up for the next poll, sooner while the terminal has focus.
    fn next_poll(&self, kit: &Kit) -> Effect {
        Effect::After {
            after: if kit.is_focused() {
                POLL_FOCUSED
            } else {
                POLL_UNFOCUSED
            },
            msg: Msg::Poll {
                generation: self.poll_generation,
            },
        }
    }

    /// Time to poll: refresh every loaded chronological log, and plan the next poll.
    pub(super) fn poll(&mut self, kit: &mut Kit) -> Effects {
        let mut polled: Vec<_> = self
            .feeds
            .iter()
            .filter(|(_, feed)| feed.is_polled())
            .map(|(&id, _)| id)
            .collect();
        polled.sort();
        let mut effects: Effects = polled
            .into_iter()
            .filter_map(|id| self.refresh(kit, id))
            .flatten()
            .collect();
        effects.push(fetch(Request::UnreadCount));
        effects.push(self.next_poll(kit));
        effects
    }
}

//! The log buffers: their names, and polling chronological logs for new commits.

use std::time::Duration;

use devnull_api::Log;

use super::{App, Effect, Kind, Request, Wake, WakePurpose};

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

impl App {
    /// Starts polling chronological logs for new commits, once.
    pub(super) fn start_polling(&mut self) -> Vec<Effect> {
        if self.polling {
            return Vec::new();
        }
        self.polling = true;
        vec![self.next_poll()]
    }

    /// The wake-up for the next poll, sooner while the terminal has focus.
    fn next_poll(&self) -> Effect {
        Effect::Wake {
            after: if self.focused {
                POLL_FOCUSED
            } else {
                POLL_UNFOCUSED
            },
            token: Wake {
                purpose: WakePurpose::Poll,
                generation: self.poll_generation,
            },
        }
    }

    /// Time to poll: refresh every loaded chronological log, and plan the next poll.
    pub(super) fn poll(&mut self) -> Vec<Effect> {
        let polled: Vec<usize> = (0..self.buffers.len())
            .filter(
                |&index| matches!(&self.buffers[index].kind, Kind::Feed(feed) if feed.is_polled()),
            )
            .collect();
        let mut effects: Vec<Effect> = polled
            .into_iter()
            .filter_map(|index| self.refresh(index))
            .flatten()
            .collect();
        effects.push(Effect::Fetch(Request::UnreadCount));
        effects.push(self.next_poll());
        effects
    }
}

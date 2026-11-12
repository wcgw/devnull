//! Writing commits (D3): the draft opens in `$EDITOR` with its context below a scissors line; once saved it is
//! counted, previewed by the server and published if confirmed. A draft not published is kept for next time.

use std::collections::HashMap;

use devnull_api::body::{self, MAX_LENGTH};
use devnull_api::model::{Commit, Preview};
use devnull_api::{Answering, Draft};

use super::actions::{Confirm, Pending};
use super::feed::Source;
use super::threads::short_hash;
use super::{App, AppMode, Effect, FetchError, NetEvent, Request};
use crate::doc::Document;
use crate::render::{commit_block, preview_block};

/// The line that ends a draft: it and everything below it are not part of the commit, so `#` above it is text.
const SCISSORS: &str = "# ------------------------ >8 ------------------------";

/// How much of the commit answered the editor's context quotes.
const EXCERPT: usize = 60;

/// A draft being written: in the editor, waiting for its preview, or for its confirmation.
#[derive(Debug)]
pub(super) struct Composing {
    answering: Answering,
    /// Whom it answers, for the confirmation.
    to: Option<String>,
    /// What the editor says it is, below the scissors.
    context: String,
}

/// The drafts not published, by what they answer.
pub(super) type Drafts = HashMap<Answering, String>;

/// The file the editor opens: `body`, then the scissors and the context.
fn template(body: &str, composing: &Composing) -> String {
    format!(
        "{body}\n\n{SCISSORS}\n# Do not modify or remove the line above.\n# Everything below it is ignored. Save and \
         quit to go on, :cq to give up.\n#\n# {}\n# Up to {MAX_LENGTH} characters; links count {}.\n",
        composing.context,
        body::LINK_COST
    )
}

/// The draft in what the editor saved: everything above the scissors line, without trailing blanks.
fn parse(saved: &str) -> &str {
    let end = saved
        .match_indices(SCISSORS)
        .find(|&(at, _)| at == 0 || saved[..at].ends_with('\n'))
        .map_or(saved.len(), |(at, _)| at);
    saved[..end].trim_end()
}

impl App {
    /// `:commit` (`c`), `:reply` (`r`) and `:quote` (`R`): opens the editor on the draft kept for that, or an
    /// empty one; a reply or quote is of the commit under the cursor.
    pub(super) fn compose(&mut self, answering: Answering) -> Result<Vec<Effect>, String> {
        let me = self
            .connection
            .handle()
            .ok_or_else(|| "Not connected".to_owned())?
            .to_owned();
        let composing = match answering {
            Answering::Nothing => Composing {
                answering,
                to: None,
                context: format!("A new commit as @{me}."),
            },
            _ => {
                let commit = self
                    .commit_under_cursor()
                    .ok_or_else(|| "No commit under the cursor".to_owned())?;
                let author = commit.author.unwrap_or_default();
                let excerpt = self.buffers[self.windows.current().buffer]
                    .doc
                    .line_text(commit.id.as_str(), 1)
                    .map(|line| line.chars().take(EXCERPT).collect::<String>());
                let verb = if matches!(answering, Answering::Reply(_)) {
                    "Replying to"
                } else {
                    "Quoting"
                };
                let what = excerpt.map_or_else(
                    || format!(" (commit {})", short_hash(&commit.id)),
                    |line| format!(": {line}"),
                );
                let answering = match answering {
                    Answering::Reply(_) => Answering::Reply(commit.id),
                    _ => Answering::Quote(commit.id),
                };
                Composing {
                    answering,
                    context: format!("{verb} @{author}{what}"),
                    to: Some(author),
                }
            }
        };
        let body = self
            .drafts
            .get(&composing.answering)
            .cloned()
            .unwrap_or_default();
        let text = template(&body, &composing);
        self.composing = Some(composing);
        Ok(vec![Effect::Edit { text }])
    }

    /// The editor closed, with what it saved or why there is nothing: an empty draft aborts, others are kept, counted
    /// and sent for a preview.
    pub(super) fn edited(&mut self, saved: Result<String, String>) -> Vec<Effect> {
        let Some(composing) = &self.composing else {
            return Vec::new();
        };
        let answering = composing.answering.clone();
        let saved = match saved {
            Ok(saved) => saved,
            Err(reason) => {
                self.composing = None;
                self.error(format!("Not committed: {reason}"));
                return Vec::new();
            }
        };
        let draft = parse(&saved).to_owned();
        if body::normalize(&draft).is_empty() {
            self.composing = None;
            self.drafts.remove(&answering);
            self.error("Aborting commit due to empty commit message.");
            return Vec::new();
        }
        self.drafts.insert(answering, draft.clone());
        if let Err(refused) = body::check(&draft) {
            self.composing = None;
            self.error(format!("Not committed: {refused}"));
            return Vec::new();
        }
        vec![Effect::Fetch(Request::Preview { body: draft })]
    }

    /// The server's preview of `draft`, or why it refuses it: asks to publish it, showing it.
    pub(super) fn previewed(
        &mut self,
        draft: String,
        result: Result<Preview, FetchError>,
    ) -> Vec<Effect> {
        let Some(composing) = self.composing.take() else {
            return Vec::new();
        };
        let preview = match result {
            Ok(preview) => preview,
            Err(FetchError::NotApproved) => return self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                self.error(format!("Not committed: {reason}"));
                return Vec::new();
            }
        };
        let kind = match composing.answering {
            Answering::Reply(_) => "reply",
            Answering::Quote(_) => "quote",
            _ => "commit",
        };
        let to = composing
            .to
            .as_deref()
            .map(|handle| format!(" → @{handle}"))
            .unwrap_or_default();
        let count = body::count(&preview.body);
        let mut shown = Document::new();
        shown.push_back("preview", preview_block(&preview));
        let draft = Draft::new(composing.answering.clone(), draft);
        self.mode = AppMode::Confirm(Confirm::new(
            format!("{kind} {count}/{MAX_LENGTH}{to}? [y]es/[e]dit/[n]o"),
            Pending::Publish { draft, composing },
            Some(shown),
        ));
        Vec::new()
    }

    /// The answer to "publish?": `y` publishes, `e` edits again, anything else keeps the draft.
    pub(super) fn publish(&mut self, key: char, draft: Draft, composing: Composing) -> Vec<Effect> {
        match key {
            'y' => vec![Effect::Fetch(Request::Publish { draft })],
            'e' => {
                let text = template(&draft.body, &composing);
                self.composing = Some(composing);
                vec![Effect::Edit { text }]
            }
            _ => {
                self.info("Kept as a draft");
                Vec::new()
            }
        }
    }

    /// `draft` published, or why not: the draft goes, and a reply joins its thread if it is open.
    pub(super) fn published(
        &mut self,
        draft: &Draft,
        result: Result<Box<Commit>, FetchError>,
        now: u64,
    ) -> Vec<Effect> {
        let commit = match result {
            Ok(commit) => commit,
            Err(FetchError::NotApproved) => return self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                self.error(format!("Not published: {reason}"));
                return Vec::new();
            }
        };
        self.drafts.remove(&draft.answering);
        if let Answering::Reply(to) = &draft.answering
            && let Some(index) = self.feed_buffer(&Source::Thread(to.clone()))
        {
            let doc = &mut self.buffers[index].doc;
            if !doc.contains(commit.id.as_str()) {
                doc.push_back(commit.id.as_str(), commit_block(&commit, now));
            }
        }
        let held = if commit.status == "pending" {
            ", held for moderation"
        } else {
            ""
        };
        self.info(format!("Published {}{held}", short_hash(&commit.id)));
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draft_is_what_is_above_the_scissors_line() {
        assert_eq!(
            parse(&format!("hi\n# a heading\n\n{SCISSORS}\n# context")),
            "hi\n# a heading"
        );
        assert_eq!(parse("no scissors left\n\n"), "no scissors left");
        assert_eq!(
            parse(&format!("quoting `{SCISSORS}` inline\n{SCISSORS}\n")),
            format!("quoting `{SCISSORS}` inline")
        );
    }
}

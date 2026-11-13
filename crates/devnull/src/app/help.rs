//! `:help`: the key bindings as the keymap has them, user maps too, in a window above the current one, as Vim shows
//! its help.

use super::windows::Axis;
use super::{App, Buffer, Kind};
use crate::doc::{Cursor, Document, Line, Span};
use crate::hl::Group;
use crate::keymap::{Keymap, Listed, Mode, Topic};

/// The help buffer's name.
const NAME: &str = "help";

impl App {
    /// `:help` and `:help {keys}`: the key bindings in the help window, opened above the current one when there is
    /// none, with the cursor on the binding of `keys` when given.
    ///
    /// # Errors
    ///
    /// Vim's E149 when nothing is bound to `keys`.
    pub(super) fn help(&mut self, keys: Option<&str>) -> Result<(), String> {
        let doc = document(&self.keymap);
        let start = match keys {
            None => None,
            Some(keys) => {
                let block = [Mode::Normal, Mode::Visual]
                    .into_iter()
                    .flat_map(|mode| [builtin_block(mode, keys), map_block(mode, keys)])
                    .find(|block| doc.contains(block.as_str()))
                    .ok_or_else(|| format!("E149: Sorry, no help for {keys}"))?;
                Some(Cursor::on_block(block.as_str()))
            }
        };
        let shown = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.kind, Kind::Help));
        let index = if let Some(index) = shown {
            self.buffers[index].doc = doc;
            index
        } else {
            self.buffers.push(Buffer {
                name: NAME.to_owned(),
                doc,
                kind: Kind::Help,
            });
            self.buffers.len() - 1
        };
        if !self.windows.go_to_buffer(index) {
            self.windows.split(Axis::Rows);
            let start = start.clone().unwrap_or_default();
            self.windows.current_mut().switch_to(index, start);
        }
        if let Some(start) = start {
            self.windows.current_mut().cursor = start;
        }
        self.follow_cursor();
        Ok(())
    }
}

/// The block of the built-in binding of `keys` in `mode`.
fn builtin_block(mode: Mode, keys: &str) -> String {
    format!("key/{}/{keys}", mode_letter(mode))
}

/// The block of the user map of `keys` in `mode`.
fn map_block(mode: Mode, keys: &str) -> String {
    format!("map/{}/{keys}", mode_letter(mode))
}

/// `mode` as `:map` shows it.
fn mode_letter(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "n",
        Mode::Visual => "v",
    }
}

/// The help: a title, the Normal-mode built-ins by topic, those only Visual mode has, then the user maps.
fn document(keymap: &Keymap) -> Document {
    let normal = keymap.listing(Mode::Normal);
    let visual = keymap.listing(Mode::Visual);
    let builtin = |listed: &&Listed| listed.topic.is_some();
    let visual_only: Vec<&Listed> = visual
        .iter()
        .filter(builtin)
        .filter(|listed| {
            !normal
                .iter()
                .any(|other| other.keys == listed.keys && other.description == listed.description)
        })
        .collect();
    let maps: Vec<(Mode, &Listed)> = [(Mode::Normal, &normal), (Mode::Visual, &visual)]
        .into_iter()
        .flat_map(|(mode, listing)| {
            listing
                .iter()
                .filter(|listed| listed.topic.is_none())
                .map(move |listed| (mode, listed))
        })
        .collect();
    let all = || normal.iter().chain(&visual);
    let keys_width = all().map(|listed| width(&listed.keys)).max().unwrap_or(0) + 2;
    let description_width = all()
        .filter(builtin)
        .map(|listed| width(&listed.description))
        .max()
        .unwrap_or(0)
        + 2;
    let row = |listed: &Listed, mode: Option<Mode>| {
        let mut spans = vec![Span::new("  ", Group::NORMAL)];
        if let Some(mode) = mode {
            spans.push(Span::new(
                format!("{} ", mode_letter(mode)),
                Group::WHICH_KEY_SEPARATOR,
            ));
        }
        spans.push(Span::new(pad(&listed.keys, keys_width), Group::WHICH_KEY));
        if let Some(command) = &listed.command {
            let description = pad(&listed.description, description_width);
            spans.push(Span::new(description, Group::WHICH_KEY_DESC));
            spans.push(Span::new(format!(":{command}"), crate::hl::CODE));
        } else {
            spans.push(Span::new(listed.description.clone(), Group::WHICH_KEY_DESC));
        }
        vec![Line::Text(spans)]
    };
    let heading = |title: &str| {
        vec![
            Line::text(""),
            Line::Text(vec![Span::new(title.to_owned(), Group::WHICH_KEY_GROUP)]),
        ]
    };

    let mut doc = Document::new();
    doc.push_back(
        "title",
        vec![
            Line::Text(vec![Span::new("Key bindings", Group::WHICH_KEY_GROUP)]),
            Line::text(format!(
                "<leader> is {}. Keys that run a command show it, to type after : or use in :map.",
                keymap.leader()
            )),
        ],
    );
    let mut topic = None;
    for listed in normal.iter().filter(builtin) {
        if listed.topic != topic {
            topic = listed.topic;
            let title = topic.map_or("", Topic::title);
            doc.push_back(format!("topic/{title}").as_str(), heading(title));
        }
        let block = builtin_block(Mode::Normal, &listed.keys);
        doc.push_back(block.as_str(), row(listed, None));
    }
    if !visual_only.is_empty() {
        doc.push_back("visual", heading("Visual mode"));
        for listed in visual_only {
            let block = builtin_block(Mode::Visual, &listed.keys);
            doc.push_back(block.as_str(), row(listed, None));
        }
    }
    if !maps.is_empty() {
        doc.push_back("maps", heading("Your maps"));
        for (mode, listed) in maps {
            let block = map_block(mode, &listed.keys);
            doc.push_back(block.as_str(), row(listed, Some(mode)));
        }
    }
    doc
}

/// The cells `text` takes.
fn width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// `text` followed by blanks up to `cells`.
fn pad(text: &str, cells: usize) -> String {
    format!("{text}{}", " ".repeat(cells.saturating_sub(width(text))))
}

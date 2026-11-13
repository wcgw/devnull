//! Pickers: a floating list narrowed by what is typed, `<C-n>`/`<C-p>` to choose, `<CR>` to open, `<Esc>` to leave.
//! The buffer picker narrows its list fuzzily; the user picker asks dev-null.sh's search once typing pauses.

#[cfg(test)]
mod tests;

use std::time::Duration;

use devnull_api::model::{AccountKind, ProfileSearch};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line as TextLine, Span as TextSpan};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::cmdline::{Cmdline, Edit, Purpose};
use super::{App, AppMode, Effect, FetchError, Request, Wake, WakePurpose};
use crate::doc::AccountRef;
use crate::ex::PickList;
use crate::hl::Group;
use crate::key::{Code, Key, Mods};

/// How long typing must pause before the user picker searches, as the Go client waits.
const SEARCH_DELAY: Duration = Duration::from_millis(120);

/// The widest a picker is, in cells.
const MAX_WIDTH: u16 = 72;

/// The most rows a picker lists.
const MAX_ROWS: u16 = 12;

/// What the query follows.
const PROMPT: &str = "> ";

/// A picker open over the windows.
#[derive(Debug)]
pub(super) struct Picker {
    list: PickList,
    /// The query, edited as a command line is.
    query: Cmdline,
    /// What can be picked, in their own order: the last shown buffer first, or as the search found them.
    choices: Vec<Choice>,
    /// The choices matching the query, best first, as indices into `choices`, each with the characters of its text
    /// that matched.
    shown: Vec<(usize, Vec<usize>)>,
    /// The index into `shown` of the chosen one.
    selected: usize,
    /// What to say while nothing is shown.
    hint: &'static str,
}

/// Something a picker can open.
#[derive(Debug, Clone)]
struct Choice {
    /// How it is listed, as text and the group it is drawn in.
    label: Vec<(String, Group)>,
    /// What the query is matched against.
    text: String,
    pick: Pick,
}

/// What picking a choice opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pick {
    /// A buffer, by index.
    Buffer(usize),
    /// A person's or an organization's profile.
    Account(AccountRef),
}

/// What a key did to a picker.
enum Outcome {
    /// Nothing the app needs to know.
    Stay,
    /// The query changed.
    Changed,
    /// `<CR>`: open the chosen one, if any.
    Picked(Option<Pick>),
    /// `<Esc>`, `<C-c>`.
    Leave,
}

/// How well `query` matches `text`, as fzf does it, and which characters of `text` matched, by index: its characters in order, ignoring case unless the query has
/// capitals. Matches at the start of words and runs of consecutive characters score higher, gaps lower. `None` when
/// `text` does not hold the query's characters in order.
fn best(query: &str, text: &str) -> Option<(i64, Vec<usize>)> {
    let smart = query.chars().any(char::is_uppercase);
    let fold = |c: char| {
        if smart {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    let text: Vec<char> = text.chars().map(fold).collect();
    let Some(&first) = query.first() else {
        return Some((0, Vec::new()));
    };
    let word_start = |i: usize| i == 0 || matches!(text[i - 1], '/' | '@' | '-' | '_' | '.' | ' ');
    // Each place the first character matches starts a greedy run; the best run counts.
    (0..text.len())
        .filter(|&i| text[i] == first)
        .filter_map(|start| {
            let mut total = 0;
            let mut at = start;
            let mut previous: Option<usize> = None;
            let mut positions = Vec::with_capacity(query.len());
            for &wanted in &query {
                let found = (at..text.len()).find(|&i| text[i] == wanted)?;
                total += 16;
                if word_start(found) {
                    total += 8;
                }
                match previous {
                    Some(previous) if found == previous + 1 => total += 8,
                    Some(previous) => {
                        total -= i64::try_from(found - previous - 1).unwrap_or(i64::MAX)
                    }
                    None => total -= i64::try_from(found).unwrap_or(i64::MAX) / 4,
                }
                previous = Some(found);
                positions.push(found);
                at = found + 1;
            }
            Some((total, positions))
        })
        // The first of the best, as `max` would pick the last.
        .fold(None, |best: Option<(i64, Vec<usize>)>, run| match best {
            Some(best) if best.0 >= run.0 => Some(best),
            _ => Some(run),
        })
}

impl Picker {
    fn new(list: PickList, choices: Vec<Choice>, hint: &'static str) -> Self {
        let mut picker = Self {
            list,
            query: Cmdline::new(Purpose::Ex, None),
            choices,
            shown: Vec::new(),
            selected: 0,
            hint,
        };
        picker.narrow();
        picker
    }

    /// The choices matching the query, best first; ties keep their order.
    fn narrow(&mut self) {
        let query = self.query.text();
        let mut scored: Vec<(usize, i64, Vec<usize>)> = self
            .choices
            .iter()
            .enumerate()
            .filter_map(|(i, choice)| {
                best(&query, &choice.text).map(|(score, matched)| (i, score, matched))
            })
            .collect();
        scored.sort_by_key(|&(_, score, _)| std::cmp::Reverse(score));
        self.shown = scored
            .into_iter()
            .map(|(i, _, matched)| (i, matched))
            .collect();
        self.selected = 0;
    }

    fn key(&mut self, key: Key) -> Outcome {
        let ctrl = |ch| Key {
            code: Code::Char(ch),
            mods: Mods::CTRL,
        };
        let count = self.shown.len();
        match key {
            k if k == ctrl('n') || k.code == Code::Down => {
                if count > 0 {
                    self.selected = (self.selected + 1) % count;
                }
                Outcome::Stay
            }
            k if k == ctrl('p') || k.code == Code::Up => {
                if count > 0 {
                    self.selected = (self.selected + count - 1) % count;
                }
                Outcome::Stay
            }
            // Unlike the command line, an empty picker stays open on <BS>.
            k if k.code == Code::Backspace && self.query.text().is_empty() => Outcome::Stay,
            k => {
                let before = self.query.text();
                match self.query.key(k, &[]) {
                    Edit::Submit(_) => Outcome::Picked(self.chosen()),
                    Edit::Cancel => Outcome::Leave,
                    Edit::Editing if self.query.text() == before => Outcome::Stay,
                    Edit::Editing => {
                        if self.list == PickList::Buffers {
                            self.narrow();
                        }
                        Outcome::Changed
                    }
                }
            }
        }
    }

    /// Pasted text, up to its first line break, goes into the query.
    fn paste(&mut self, text: &str) -> Outcome {
        self.query.paste(text);
        if self.list == PickList::Buffers {
            self.narrow();
        }
        Outcome::Changed
    }

    fn chosen(&self) -> Option<Pick> {
        let &(index, _) = self.shown.get(self.selected)?;
        Some(self.choices[index].pick.clone())
    }

    /// The search term the query stands for; empty searches nothing.
    fn term(&self) -> String {
        self.query.text().trim().to_owned()
    }
}

impl App {
    /// `:pick {list} [query]`.
    pub(super) fn pick(&mut self, list: PickList, query: &str) -> Vec<Effect> {
        let mut picker = match list {
            PickList::Buffers => self.buffer_picker(),
            PickList::Users => {
                Picker::new(list, Vec::new(), "Type to find people and organizations")
            }
        };
        if !query.is_empty() {
            picker.query.paste(query);
            picker.narrow();
        }
        let term = picker.term();
        self.mode = AppMode::Picker(picker);
        if list == PickList::Users && !term.is_empty() {
            return vec![Effect::Fetch(Request::SearchProfiles { term })];
        }
        Vec::new()
    }

    /// The listed buffers, the one shown first, then the others from the last shown; it starts on the second.
    fn buffer_picker(&self) -> Picker {
        let current = self.windows.current().buffer;
        let mut order: Vec<usize> = std::iter::once(current)
            .chain(self.recent.iter().copied())
            .chain(self.listed())
            .filter(|&index| self.buffers[index].is_listed())
            .collect();
        let mut seen = std::collections::HashSet::new();
        order.retain(|index| seen.insert(*index));
        let choices = order
            .into_iter()
            .map(|index| {
                let name = self.buffers[index].name.clone();
                let group = if index == current {
                    crate::hl::TIME
                } else {
                    Group::NORMAL_FLOAT
                };
                Choice {
                    label: vec![(name.clone(), group)],
                    text: name,
                    pick: Pick::Buffer(index),
                }
            })
            .collect();
        let mut picker = Picker::new(PickList::Buffers, choices, "No matching buffer");
        if picker.shown.len() > 1 {
            picker.selected = 1;
        }
        picker
    }

    /// A key while a picker is open.
    pub(super) fn picker_key(&mut self, key: Key) -> Vec<Effect> {
        let AppMode::Picker(picker) = &mut self.mode else {
            return Vec::new();
        };
        let outcome = picker.key(key);
        self.picked(outcome)
    }

    /// Text pasted while a picker is open.
    pub(super) fn picker_paste(&mut self, text: &str) -> Vec<Effect> {
        let AppMode::Picker(picker) = &mut self.mode else {
            return Vec::new();
        };
        let outcome = picker.paste(text);
        self.picked(outcome)
    }

    /// What the open picker does about what a key or a paste did to it.
    fn picked(&mut self, outcome: Outcome) -> Vec<Effect> {
        let AppMode::Picker(picker) = &mut self.mode else {
            return Vec::new();
        };
        match outcome {
            Outcome::Stay => Vec::new(),
            Outcome::Changed if picker.list == PickList::Users => {
                self.search_generation += 1;
                if picker.term().is_empty() {
                    picker.choices.clear();
                    picker.narrow();
                    picker.hint = "Type to find people and organizations";
                    return Vec::new();
                }
                picker.hint = "Searching…";
                vec![Effect::Wake {
                    after: SEARCH_DELAY,
                    token: Wake {
                        purpose: WakePurpose::Search,
                        generation: self.search_generation,
                    },
                }]
            }
            Outcome::Changed => Vec::new(),
            Outcome::Leave => {
                self.mode = AppMode::Normal;
                Vec::new()
            }
            Outcome::Picked(pick) => {
                self.mode = AppMode::Normal;
                match pick {
                    Some(Pick::Buffer(index)) => self.show_buffer(index),
                    Some(Pick::Account(account)) => self.open_profile(account),
                    None => Vec::new(),
                }
            }
        }
    }

    /// Typing paused in the user picker: search for what it says.
    pub(super) fn search_people(&mut self) -> Vec<Effect> {
        match &self.mode {
            AppMode::Picker(picker)
                if picker.list == PickList::Users && !picker.term().is_empty() =>
            {
                vec![Effect::Fetch(Request::SearchProfiles {
                    term: picker.term(),
                })]
            }
            _ => Vec::new(),
        }
    }

    /// What a search for `term` found, for the user picker if it still asks for that.
    pub(super) fn found(
        &mut self,
        term: &str,
        result: Result<ProfileSearch, FetchError>,
    ) -> Vec<Effect> {
        let AppMode::Picker(picker) = &mut self.mode else {
            return Vec::new();
        };
        if picker.list != PickList::Users || picker.term() != term {
            return Vec::new();
        }
        let found = match result {
            Ok(found) => found,
            Err(FetchError::NotApproved) => {
                self.mode = AppMode::Normal;
                return self.net(super::NetEvent::NotApproved);
            }
            Err(FetchError::Failed(_)) => {
                picker.choices.clear();
                picker.narrow();
                picker.hint = "Cannot search now";
                return Vec::new();
            }
        };
        let account = |name: &str, handle: &str, kind: AccountKind| {
            let mut label = Vec::new();
            if !name.is_empty() {
                label.push((name.to_owned(), crate::hl::AUTHOR));
                if kind == AccountKind::Organization {
                    label.push((" (org)".to_owned(), crate::hl::TIME));
                }
                label.push((" ".to_owned(), Group::NORMAL_FLOAT));
            }
            label.push((format!("@{handle}"), crate::hl::HANDLE));
            Choice {
                label,
                text: String::new(),
                pick: Pick::Account(AccountRef::new(handle, Some(kind))),
            }
        };
        picker.choices = found
            .people
            .iter()
            .map(|person| account(&person.name, &person.handle, AccountKind::User))
            .chain(
                found
                    .organizations
                    .iter()
                    .map(|org| account(&org.name, &org.handle, AccountKind::Organization)),
            )
            .collect();
        // The server ranks them: every one is shown, in its order.
        picker.shown = (0..picker.choices.len()).map(|i| (i, Vec::new())).collect();
        picker.selected = 0;
        picker.hint = "Nobody found";
        Vec::new()
    }

    /// Draws `picker` in the middle of the screen, and returns where the cursor goes: in its query.
    pub(super) fn render_picker(&self, frame: &mut Frame<'_>, picker: &Picker) -> Position {
        let (width, height) = self.size;
        let card_width = MAX_WIDTH.min(width.saturating_sub(4)).max(10).min(width);
        let rows = u16::try_from(picker.shown.len())
            .unwrap_or(u16::MAX)
            .clamp(1, MAX_ROWS);
        let card_height = (rows + 3).min(height.saturating_sub(2)).max(3).min(height);
        let area = Rect::new(
            (width - card_width) / 2,
            height.saturating_sub(card_height) / 3,
            card_width,
            card_height,
        );
        let title = match picker.list {
            PickList::Buffers => " Buffers ",
            PickList::Users => " People and organizations ",
        };
        let block = Block::bordered()
            .title(title)
            .border_style(self.theme.style(Group::FLOAT_BORDER))
            .style(self.theme.style(Group::NORMAL_FLOAT));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let prompt = format!("{PROMPT}{}", picker.query.text());
        frame.render_widget(
            Paragraph::new(prompt),
            Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
        );
        let list = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let visible = usize::from(list.height);
        // Scrolled so the chosen one is in view.
        let first = picker.selected.saturating_sub(visible.saturating_sub(1));
        let lines: Vec<TextLine<'_>> = if picker.shown.is_empty() {
            vec![TextLine::from(vec![
                TextSpan::raw("  "),
                TextSpan::styled(picker.hint, self.theme.style(crate::hl::TIME)),
            ])]
        } else {
            picker
                .shown
                .iter()
                .skip(first)
                .take(visible)
                .map(|(index, matched)| {
                    let mut spans = vec![TextSpan::raw("  ")];
                    // The label's characters, those that matched in `PmenuMatch`.
                    let mut at = 0;
                    for (text, group) in &picker.choices[*index].label {
                        for ch in text.chars() {
                            let group = if matched.contains(&at) {
                                Group::PMENU_MATCH
                            } else {
                                *group
                            };
                            spans.push(TextSpan::styled(ch.to_string(), self.theme.style(group)));
                            at += 1;
                        }
                    }
                    TextLine::from(spans)
                })
                .collect()
        };
        frame.render_widget(Paragraph::new(lines), list);
        if !picker.shown.is_empty() {
            // Over the labels' own colours, as Visual mode's selection.
            let row = u16::try_from(picker.selected - first).unwrap_or(0);
            if row < list.height {
                frame.buffer_mut().set_style(
                    Rect::new(list.x, list.y + row, list.width, 1),
                    self.theme.style(Group::VISUAL),
                );
            }
        }
        let x = inner.x + u16::try_from(PROMPT.width() + picker.query.cursor_cells()).unwrap_or(0);
        Position::new(x.min(inner.right().saturating_sub(1)), inner.y)
    }
}

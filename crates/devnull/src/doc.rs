//! The text model behind every buffer (D4): the kit's document model (its D3), with what `dn`'s text refers to.

pub use wcgw_kit::doc::{
    BlockKey, CaseMode, Cursor, Direction, FoldOp, Folds, Motion, Pattern, PatternError,
    PictureRow, ScreenPos, SearchOutcome, Selection, SelectionKind,
};

use devnull_api::model::AccountKind;

/// A read-only document of keyed blocks of lines, whose text refers to [`Target`]s.
pub type Document = wcgw_kit::doc::Document<Target>;
/// One line of a block before wrapping.
pub type Line = wcgw_kit::doc::Line<Target>;
/// A run of text sharing one highlight group and one target.
pub type Span = wcgw_kit::doc::Span<Target>;
/// What a block holds: its lines, and the folds over them.
pub type Content = wcgw_kit::doc::Content<Target>;
/// A foldable range of a block's lines.
pub type Fold = wcgw_kit::doc::Fold<Target>;
/// A document wrapped into screen rows for a width.
pub type Layout<'d> = wcgw_kit::doc::Layout<'d, Target>;

/// Something the text refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A web link, opened in the browser.
    Url(String),
    /// A person or an organization.
    Account(AccountRef),
    /// A commit.
    Commit {
        /// Its id.
        id: String,
        /// Who wrote it; `None` for a deleted commit.
        author: Option<AccountRef>,
        /// Its page on the web; `None` for a deleted commit.
        url: Option<String>,
        /// Whether the connected account starred it.
        starred: bool,
    },
    /// The card the server made for a link, opened in the browser like the link.
    LinkCard(LinkCard),
    /// A buffer, by name, such as a profile's list of followers.
    Buffer(String),
}

/// What the server says about a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCard {
    /// The link.
    pub url: String,
    /// The page's title; may be empty.
    pub title: String,
    /// The site's name; may be empty.
    pub site: String,
    /// What the page says it is about; may be empty.
    pub description: String,
}

/// A person or an organization, as text refers to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRef {
    /// The handle, without the `@`.
    pub handle: String,
    /// Whether it is a person or an organization; `None` when the text does not say.
    pub kind: Option<AccountKind>,
}

impl AccountRef {
    /// `handle`, of `kind`; a kind this version does not know counts as not saying.
    pub fn new(handle: impl Into<String>, kind: Option<AccountKind>) -> Self {
        Self {
            handle: handle.into(),
            kind: kind.filter(|kind| *kind != AccountKind::Other),
        }
    }
}

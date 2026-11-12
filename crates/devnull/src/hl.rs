//! Highlight groups: the names text is styled by, resolved to colours by the active colorscheme (D6).

/// A named highlight group. `:hi` overrides a group by name; the colorscheme gives every group its default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Group {
    /// Plain text.
    #[default]
    Normal,
    /// A user or organization handle.
    Handle,
}

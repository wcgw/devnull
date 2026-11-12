//! The tokyonight colorschemes, generated from [folke/tokyonight.nvim](https://github.com/folke/tokyonight.nvim)
//! (Apache-2.0, Copyright Folke Lemaitre) `extras/lua/tokyonight_*.lua` at commit `cdc07ac78467a233fd62c493de29a17e0cf2b2b6`.
//! Regenerate rather than edit by hand.

use ratatui::style::Color;

use super::{Attrs, Group, Spec};

/// tokyonight-night.
pub(super) const NIGHT: &[(Group, Spec)] = &[
    (
        Group::Normal,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: Some(Color::Rgb(0x1a, 0x1b, 0x26)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NonText,
        Spec {
            fg: Some(Color::Rgb(0x54, 0x5c, 0x7e)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::EndOfBuffer,
        Spec {
            fg: Some(Color::Rgb(0x1a, 0x1b, 0x26)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLine,
        Spec {
            fg: Some(Color::Rgb(0xa9, 0xb1, 0xd6)),
            bg: Some(Color::Rgb(0x16, 0x16, 0x1e)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineNC,
        Spec {
            fg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            bg: Some(Color::Rgb(0x16, 0x16, 0x1e)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineSel,
        Spec {
            fg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            bg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineFill,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WinSeparator,
        Spec {
            fg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::ErrorMsg,
        Spec {
            fg: Some(Color::Rgb(0xdb, 0x4b, 0x4b)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WarningMsg,
        Spec {
            fg: Some(Color::Rgb(0xe0, 0xaf, 0x68)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::MoreMsg,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Visual,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x28, 0x34, 0x57)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Folded,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NormalFloat,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: Some(Color::Rgb(0x16, 0x16, 0x1e)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::FloatBorder,
        Spec {
            fg: Some(Color::Rgb(0x27, 0xa1, 0xb9)),
            bg: Some(Color::Rgb(0x16, 0x16, 0x1e)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKey,
        Spec {
            fg: Some(Color::Rgb(0x7d, 0xcf, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeySeparator,
        Spec {
            fg: Some(Color::Rgb(0x56, 0x5f, 0x89)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyDesc,
        Spec {
            fg: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyGroup,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineModeNormal,
        Spec {
            fg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            bg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeVisual,
        Spec {
            fg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            bg: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeCommand,
        Spec {
            fg: Some(Color::Rgb(0x15, 0x16, 0x1e)),
            bg: Some(Color::Rgb(0xe0, 0xaf, 0x68)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Handle,
        Spec {
            fg: Some(Color::Rgb(0x7d, 0xcf, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Author,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Time,
        Spec {
            fg: Some(Color::Rgb(0x56, 0x5f, 0x89)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Hash,
        Spec {
            fg: Some(Color::Rgb(0x73, 0x7a, 0xa2)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Stats,
        Spec {
            fg: Some(Color::Rgb(0xa9, 0xb1, 0xd6)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Link,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::UNDERLINE,
        },
    ),
    (
        Group::Code,
        Spec {
            fg: Some(Color::Rgb(0x9e, 0xce, 0x6a)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
];

/// tokyonight-storm.
pub(super) const STORM: &[(Group, Spec)] = &[
    (
        Group::Normal,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: Some(Color::Rgb(0x24, 0x28, 0x3b)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NonText,
        Spec {
            fg: Some(Color::Rgb(0x54, 0x5c, 0x7e)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::EndOfBuffer,
        Spec {
            fg: Some(Color::Rgb(0x24, 0x28, 0x3b)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLine,
        Spec {
            fg: Some(Color::Rgb(0xa9, 0xb1, 0xd6)),
            bg: Some(Color::Rgb(0x1f, 0x23, 0x35)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineNC,
        Spec {
            fg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            bg: Some(Color::Rgb(0x1f, 0x23, 0x35)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineSel,
        Spec {
            fg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            bg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineFill,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WinSeparator,
        Spec {
            fg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::ErrorMsg,
        Spec {
            fg: Some(Color::Rgb(0xdb, 0x4b, 0x4b)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WarningMsg,
        Spec {
            fg: Some(Color::Rgb(0xe0, 0xaf, 0x68)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::MoreMsg,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Visual,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x2e, 0x3c, 0x64)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Folded,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NormalFloat,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: Some(Color::Rgb(0x1f, 0x23, 0x35)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::FloatBorder,
        Spec {
            fg: Some(Color::Rgb(0x29, 0xa4, 0xbd)),
            bg: Some(Color::Rgb(0x1f, 0x23, 0x35)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKey,
        Spec {
            fg: Some(Color::Rgb(0x7d, 0xcf, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeySeparator,
        Spec {
            fg: Some(Color::Rgb(0x56, 0x5f, 0x89)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyDesc,
        Spec {
            fg: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyGroup,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineModeNormal,
        Spec {
            fg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            bg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeVisual,
        Spec {
            fg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            bg: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeCommand,
        Spec {
            fg: Some(Color::Rgb(0x1d, 0x20, 0x2f)),
            bg: Some(Color::Rgb(0xe0, 0xaf, 0x68)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Handle,
        Spec {
            fg: Some(Color::Rgb(0x7d, 0xcf, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Author,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0xca, 0xf5)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Time,
        Spec {
            fg: Some(Color::Rgb(0x56, 0x5f, 0x89)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Hash,
        Spec {
            fg: Some(Color::Rgb(0x73, 0x7a, 0xa2)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Stats,
        Spec {
            fg: Some(Color::Rgb(0xa9, 0xb1, 0xd6)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Link,
        Spec {
            fg: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            bg: None,
            sp: None,
            attrs: Attrs::UNDERLINE,
        },
    ),
    (
        Group::Code,
        Spec {
            fg: Some(Color::Rgb(0x9e, 0xce, 0x6a)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
];

/// tokyonight-moon.
pub(super) const MOON: &[(Group, Spec)] = &[
    (
        Group::Normal,
        Spec {
            fg: Some(Color::Rgb(0xc8, 0xd3, 0xf5)),
            bg: Some(Color::Rgb(0x22, 0x24, 0x36)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NonText,
        Spec {
            fg: Some(Color::Rgb(0x54, 0x5c, 0x7e)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::EndOfBuffer,
        Spec {
            fg: Some(Color::Rgb(0x22, 0x24, 0x36)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLine,
        Spec {
            fg: Some(Color::Rgb(0x82, 0x8b, 0xb8)),
            bg: Some(Color::Rgb(0x1e, 0x20, 0x30)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineNC,
        Spec {
            fg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            bg: Some(Color::Rgb(0x1e, 0x20, 0x30)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineSel,
        Spec {
            fg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            bg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineFill,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WinSeparator,
        Spec {
            fg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::ErrorMsg,
        Spec {
            fg: Some(Color::Rgb(0xc5, 0x3b, 0x53)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WarningMsg,
        Spec {
            fg: Some(Color::Rgb(0xff, 0xc7, 0x77)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::MoreMsg,
        Spec {
            fg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Visual,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0x2d, 0x3f, 0x76)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Folded,
        Spec {
            fg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            bg: Some(Color::Rgb(0x3b, 0x42, 0x61)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NormalFloat,
        Spec {
            fg: Some(Color::Rgb(0xc8, 0xd3, 0xf5)),
            bg: Some(Color::Rgb(0x1e, 0x20, 0x30)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::FloatBorder,
        Spec {
            fg: Some(Color::Rgb(0x58, 0x9e, 0xd7)),
            bg: Some(Color::Rgb(0x1e, 0x20, 0x30)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKey,
        Spec {
            fg: Some(Color::Rgb(0x86, 0xe1, 0xfc)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeySeparator,
        Spec {
            fg: Some(Color::Rgb(0x63, 0x6d, 0xa6)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyDesc,
        Spec {
            fg: Some(Color::Rgb(0xc0, 0x99, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyGroup,
        Spec {
            fg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineModeNormal,
        Spec {
            fg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            bg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeVisual,
        Spec {
            fg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            bg: Some(Color::Rgb(0xc0, 0x99, 0xff)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeCommand,
        Spec {
            fg: Some(Color::Rgb(0x1b, 0x1d, 0x2b)),
            bg: Some(Color::Rgb(0xff, 0xc7, 0x77)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Handle,
        Spec {
            fg: Some(Color::Rgb(0x86, 0xe1, 0xfc)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Author,
        Spec {
            fg: Some(Color::Rgb(0xc8, 0xd3, 0xf5)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Time,
        Spec {
            fg: Some(Color::Rgb(0x63, 0x6d, 0xa6)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Hash,
        Spec {
            fg: Some(Color::Rgb(0x73, 0x7a, 0xa2)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Stats,
        Spec {
            fg: Some(Color::Rgb(0x82, 0x8b, 0xb8)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Link,
        Spec {
            fg: Some(Color::Rgb(0x82, 0xaa, 0xff)),
            bg: None,
            sp: None,
            attrs: Attrs::UNDERLINE,
        },
    ),
    (
        Group::Code,
        Spec {
            fg: Some(Color::Rgb(0xc3, 0xe8, 0x8d)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
];

/// tokyonight-day.
pub(super) const DAY: &[(Group, Spec)] = &[
    (
        Group::Normal,
        Spec {
            fg: Some(Color::Rgb(0x37, 0x60, 0xbf)),
            bg: Some(Color::Rgb(0xe1, 0xe2, 0xe7)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NonText,
        Spec {
            fg: Some(Color::Rgb(0x89, 0x90, 0xb3)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::EndOfBuffer,
        Spec {
            fg: Some(Color::Rgb(0xe1, 0xe2, 0xe7)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLine,
        Spec {
            fg: Some(Color::Rgb(0x61, 0x72, 0xb0)),
            bg: Some(Color::Rgb(0xd0, 0xd5, 0xe3)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineNC,
        Spec {
            fg: Some(Color::Rgb(0xa8, 0xae, 0xcb)),
            bg: Some(Color::Rgb(0xd0, 0xd5, 0xe3)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineSel,
        Spec {
            fg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            bg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::TabLineFill,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WinSeparator,
        Spec {
            fg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::ErrorMsg,
        Spec {
            fg: Some(Color::Rgb(0xc6, 0x43, 0x43)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WarningMsg,
        Spec {
            fg: Some(Color::Rgb(0x8c, 0x6c, 0x3e)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::MoreMsg,
        Spec {
            fg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Visual,
        Spec {
            fg: None,
            bg: Some(Color::Rgb(0xb7, 0xc1, 0xe3)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Folded,
        Spec {
            fg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            bg: Some(Color::Rgb(0xa8, 0xae, 0xcb)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::NormalFloat,
        Spec {
            fg: Some(Color::Rgb(0x37, 0x60, 0xbf)),
            bg: Some(Color::Rgb(0xd0, 0xd5, 0xe3)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::FloatBorder,
        Spec {
            fg: Some(Color::Rgb(0x40, 0x94, 0xa3)),
            bg: Some(Color::Rgb(0xd0, 0xd5, 0xe3)),
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKey,
        Spec {
            fg: Some(Color::Rgb(0x00, 0x71, 0x97)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeySeparator,
        Spec {
            fg: Some(Color::Rgb(0x84, 0x8c, 0xb5)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyDesc,
        Spec {
            fg: Some(Color::Rgb(0x98, 0x54, 0xf1)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::WhichKeyGroup,
        Spec {
            fg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::StatusLineModeNormal,
        Spec {
            fg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            bg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeVisual,
        Spec {
            fg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            bg: Some(Color::Rgb(0x98, 0x54, 0xf1)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::StatusLineModeCommand,
        Spec {
            fg: Some(Color::Rgb(0xb4, 0xb5, 0xb9)),
            bg: Some(Color::Rgb(0x8c, 0x6c, 0x3e)),
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Handle,
        Spec {
            fg: Some(Color::Rgb(0x00, 0x71, 0x97)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Author,
        Spec {
            fg: Some(Color::Rgb(0x37, 0x60, 0xbf)),
            bg: None,
            sp: None,
            attrs: Attrs::BOLD,
        },
    ),
    (
        Group::Time,
        Spec {
            fg: Some(Color::Rgb(0x84, 0x8c, 0xb5)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Hash,
        Spec {
            fg: Some(Color::Rgb(0x68, 0x70, 0x9a)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Stats,
        Spec {
            fg: Some(Color::Rgb(0x61, 0x72, 0xb0)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
    (
        Group::Link,
        Spec {
            fg: Some(Color::Rgb(0x2e, 0x7d, 0xe9)),
            bg: None,
            sp: None,
            attrs: Attrs::UNDERLINE,
        },
    ),
    (
        Group::Code,
        Spec {
            fg: Some(Color::Rgb(0x58, 0x75, 0x39)),
            bg: None,
            sp: None,
            attrs: Attrs::NONE,
        },
    ),
];

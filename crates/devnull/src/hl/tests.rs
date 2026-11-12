//! Highlight groups, colorschemes and `:hi`, through the public `Theme` interface.

use ratatui::style::{Color, Modifier};

use super::*;

fn hi(theme: &mut Theme, line: &str) -> Result<Option<String>, String> {
    let mut words = line.split_whitespace();
    let group = words.next().unwrap_or_default();
    let attrs: Vec<(String, String)> = words
        .map(|word| {
            let (key, value) = word.split_once('=').unwrap_or((word, ""));
            (key.to_owned(), value.to_owned())
        })
        .collect();
    theme.highlight(group, &attrs)
}

#[test]
fn the_default_theme_is_tokyonight_night() {
    let theme = Theme::default();

    assert_eq!(theme.name(), "tokyonight-night");
    let normal = theme.style(Group::Normal);
    assert_eq!(normal.fg, Some(Color::Rgb(0xc0, 0xca, 0xf5)));
    assert_eq!(normal.bg, Some(Color::Rgb(0x1a, 0x1b, 0x26)));
}

#[test]
fn colorschemes_are_found_by_name_and_tokyonight_means_night() {
    let day = Theme::named("tokyonight-day").expect("day exists");
    assert_ne!(
        day.style(Group::Normal).bg,
        Theme::default().style(Group::Normal).bg
    );
    assert_eq!(
        Theme::named("tokyonight").map(|t| t.style(Group::Normal)),
        Ok(Theme::default().style(Group::Normal))
    );
    assert!(Theme::named("default").is_ok());
    assert_eq!(
        Theme::named("solarized").err(),
        Some("E185: Cannot find color scheme 'solarized'".to_owned())
    );
}

#[test]
fn hi_sets_colors_and_attributes_keeping_the_rest() {
    let mut theme = Theme::default();
    let before = theme.style(Group::StatusLine);

    assert_eq!(
        hi(&mut theme, "statusline guifg=#ff0000 gui=bold,italic"),
        Ok(None)
    );

    let after = theme.style(Group::StatusLine);
    assert_eq!(after.fg, Some(Color::Rgb(0xff, 0, 0)));
    assert_eq!(after.bg, before.bg);
    assert!(
        after
            .add_modifier
            .contains(Modifier::BOLD | Modifier::ITALIC)
    );
    assert_eq!(hi(&mut theme, "StatusLine bg=NONE ctermbg=0"), Ok(None));
    assert_eq!(theme.style(Group::StatusLine).bg, None);
}

#[test]
fn hi_with_only_a_group_shows_it() {
    let mut theme = Theme::default();
    let _ = hi(&mut theme, "Visual gui=reverse");

    assert_eq!(
        hi(&mut theme, "Visual"),
        Ok(Some(
            "Visual         xxx gui=reverse guibg=#283457".to_owned()
        ))
    );
    assert_eq!(
        hi(&mut theme, "DevnullHandle"),
        Ok(Some("DevnullHandle  xxx guifg=#7dcfff".to_owned()))
    );
}

#[test]
fn hi_reports_vim_errors() {
    let mut theme = Theme::default();

    assert_eq!(
        hi(&mut theme, "Nope guifg=#fff000"),
        Err("E411: Highlight group not found: Nope".to_owned())
    );
    assert_eq!(
        hi(&mut theme, "Normal guifg=#zzz"),
        Err("E254: Cannot allocate color #zzz".to_owned())
    );
    assert_eq!(
        hi(&mut theme, "Normal frob=1"),
        Err("E423: Illegal argument: frob=1".to_owned())
    );
    assert_eq!(
        hi(&mut theme, "Normal gui=wobbly"),
        Err("E418: Illegal value: wobbly".to_owned())
    );
}

#[test]
fn hi_clear_goes_back_to_the_colorscheme() {
    let mut theme = Theme::named("tokyonight-storm").expect("storm exists");
    let original = theme.style(Group::Visual);
    let _ = hi(&mut theme, "Visual guibg=#000000");

    theme.clear();

    assert_eq!(theme.style(Group::Visual), original);
    assert_eq!(theme.name(), "tokyonight-storm");
}

#[test]
fn named_terminal_colors_are_accepted() {
    let mut theme = Theme::named("default").expect("default exists");

    assert_eq!(
        hi(&mut theme, "ErrorMsg guifg=Red guibg=darkgray"),
        Ok(None)
    );

    assert_eq!(theme.style(Group::ErrorMsg).fg, Some(Color::Red));
    assert_eq!(theme.style(Group::ErrorMsg).bg, Some(Color::DarkGray));
}

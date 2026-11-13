//! `dn`'s ex commands (D5): every action on what dev-null.sh shows has one, so `:map` can reach anything a key
//! does.

use devnull_api::Answering;
use wcgw_kit::shell::Command;

use super::{Dn, Kit};

/// Whom `:commit` and its kin post as: the one handle in `args`, `@` or not; `None` without one.
fn author(args: &str) -> Result<Option<String>, String> {
    match args.split_whitespace().collect::<Vec<_>>()[..] {
        [] => Ok(None),
        [handle] => Ok(Some(handle.trim_start_matches('@').to_owned())),
        _ => Err(format!("E475: Invalid argument: {args}")),
    }
}

/// `args`, if not empty.
fn optional(args: &str) -> Option<String> {
    (!args.is_empty()).then(|| args.to_owned())
}

/// Adds `dn`'s commands to `kit`, with the shortest abbreviation Vim would accept for each.
pub(super) fn add(kit: &mut Kit) {
    let commands = [
        Command::<Dn>::new("profile", 4, |dn, kit, line| {
            dn.profile(kit, optional(line.args))
        })
        .jumps()
        .help("a profile; alone, of the account under the cursor"),
        Command::<Dn>::new("browse", 3, |dn, kit, line| match line.args {
            "" => dn.browse(kit, false),
            "link" => dn.browse(kit, true),
            args => Err(format!("E475: Invalid argument: {args}")),
        })
        .help("the commit or account on the web; with link, the link under the cursor, or one of the commit's")
        .complete(|_, _, before, _| {
            if before.is_empty() {
                vec!["link".to_owned()]
            } else {
                Vec::new()
            }
        }),
        Command::<Dn>::new("yanklink", 5, |_, kit, _| Dn::yank_link(kit))
            .help("yank the link under the cursor, else the one :browse opens"),
        Command::<Dn>::new("details", 3, |dn, kit, _| dn.hover(kit))
            .help("a card about what is under the cursor; K again goes into it"),
        Command::<Dn>::new("star", 4, |dn, kit, _| dn.star(kit))
            .help("star the commit, or take the star back"),
        Command::<Dn>::new("commit", 4, |dn, kit, line| {
            dn.compose(kit, Answering::Nothing, author(line.args)?)
        })
        .help("write a commit, as an organization when named"),
        Command::<Dn>::new("reply", 3, |dn, kit, line| {
            dn.compose(kit, Answering::Reply(String::new()), author(line.args)?)
        })
        .help("reply to the commit"),
        Command::<Dn>::new("quote", 3, |dn, kit, line| {
            dn.compose(kit, Answering::Quote(String::new()), author(line.args)?)
        })
        .help("quote the commit"),
        Command::<Dn>::new("delete", 3, |dn, kit, _| dn.delete(kit)).help("delete your own commit"),
        Command::<Dn>::new("follow", 3, |dn, kit, line| {
            dn.follow(kit, handle(line.args), true)
        })
        .help("follow the account named or under the cursor"),
        Command::<Dn>::new("unfollow", 3, |dn, kit, line| {
            dn.follow(kit, handle(line.args), false)
        })
        .help("stop following it"),
        Command::<Dn>::new("connect", 4, |dn, kit, line| {
            dn.connect(kit, optional(line.args))
        })
        .help("connect this device; with a link, finish connecting"),
        Command::<Dn>::new("agree", 5, |dn, _, _| dn.agree()).help("accept dev-null.sh's terms"),
    ];
    for command in commands {
        kit.add_command(command);
    }
    kit.add_picker("users", |_, _| super::picker::users());
}

/// The handle `:follow` names, `@` or not; `None` for what is under the cursor.
fn handle(args: &str) -> Option<String> {
    (!args.is_empty()).then(|| args.trim_start_matches('@').to_owned())
}

//! The user picker (`<leader>/`, `:pick users`): people and organizations, as dev-null.sh's search finds them once
//! typing pauses.

use devnull_api::model::{AccountKind, ProfileSearch};
use wcgw_kit::shell::{Choice, Pick, Picker};

use super::{Dn, Effects, FetchError, Kit, NetEvent, Request, fetch};
use crate::doc::{AccountRef, Target};
use crate::hl::Group;

/// What the user picker says while there is nothing to search for.
const IDLE: &str = "Type to find people and organizations";

/// The user picker, its title saying what it searches.
pub(super) fn users() -> Picker<Dn> {
    Picker::search("People and organizations", IDLE, |_, _, term| {
        vec![fetch(Request::SearchProfiles { term })]
    })
}

impl Dn {
    /// What a search for `term` found, for the user picker if it still asks for that.
    pub(super) fn found(
        &mut self,
        kit: &mut Kit,
        term: &str,
        result: Result<ProfileSearch, FetchError>,
    ) -> Effects {
        let Some(picker) = kit.picker_mut() else {
            return Vec::new();
        };
        if !picker.is_searching() || picker.term() != term {
            return Vec::new();
        }
        let found = match result {
            Ok(found) => found,
            Err(FetchError::NotApproved) => {
                kit.close_picker();
                return self.net(kit, NetEvent::NotApproved);
            }
            Err(FetchError::Failed(_)) => {
                picker.found(term, Vec::new(), "Cannot search now");
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
                pick: Pick::Target(Target::Account(AccountRef::new(handle, Some(kind)))),
            }
        };
        let choices = found
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
        picker.found(term, choices, "Nobody found");
        Vec::new()
    }
}

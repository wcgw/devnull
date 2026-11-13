//! `dn` in the kit's runtime (its D2): the terminal, the event loop and the effects are the kit's; `dn` brings its
//! network side.

use std::io;
use std::path::PathBuf;

use wcgw_kit::runtime;

use crate::app::Dn;
use crate::net::Net;

/// Runs `dn` until it quits, restoring the terminal however it ends.
///
/// # Errors
///
/// If the terminal cannot be set up, read from or drawn to.
pub async fn run() -> io::Result<()> {
    let (inbox, mailbox) = runtime::channel();
    let net = Net::new(
        runtime::config_dir("devnull").map(|dir| dir.join("device_ed25519")),
        std::env::var_os("SSH_AUTH_SOCK").map(PathBuf::from),
        inbox.clone(),
    );
    let (dn, kit) = Dn::session();
    runtime::run("devnull", dn, kit, net, (inbox, mailbox)).await
}

//! `dn`: a Neovim-flavoured terminal client for [dev-null.sh](https://dev-null.sh).

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match devnull::runtime::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dn: {error}");
            ExitCode::FAILURE
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-collab-server`: the self-hosted collaboration relay. See the library documentation
//! and `apps/collab-server/README.md`.
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use kiln_collab_server::{Relay, SharedToken, VERSION};

const USAGE: &str = "\
kiln-collab-server: the Kiln collaboration relay

Usage: kiln-collab-server --token <TOKEN> [--bind <ADDR>] [--data <DIR>]

Options:
  --bind <ADDR>    Address to listen on [default: 127.0.0.1:7070]
  --data <DIR>     Directory of the room database [default: ./kiln-collab-data]
  --token <TOKEN>  The shared token clients must present (or KILN_COLLAB_TOKEN)
  -h, --help       Print this help
  -V, --version    Print the version

Rooms are served at ws://<ADDR>/rooms/<ROOM>; GET /health answers ok.
The shared token admits anyone who holds it. Serve the relay behind TLS and
on a network you trust; it is not built for the open internet.
";

struct Args {
    bind: SocketAddr,
    data: PathBuf,
    token: String,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Args>, String> {
    let mut bind: SocketAddr = ([127, 0, 0, 1], 7070).into();
    let mut data = PathBuf::from("kiln-collab-data");
    let mut token = std::env::var("KILN_COLLAB_TOKEN").ok();
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("kiln-collab-server {VERSION}");
                return Ok(None);
            }
            "--bind" => {
                let v = value("--bind")?;
                bind = v
                    .parse()
                    .map_err(|_| format!("--bind: not an address: {v}"))?;
            }
            "--data" => data = PathBuf::from(value("--data")?),
            "--token" => token = Some(value("--token")?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let token = token
        .filter(|t| !t.is_empty())
        .ok_or("a token is required: --token <TOKEN> or KILN_COLLAB_TOKEN")?;
    Ok(Some(Args { bind, data, token }))
}

/// Ctrl+C, or SIGTERM (what `docker stop` and service managers send).
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(Some(a)) => a,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kiln-collab-server: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("kiln-collab-server: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let relay = Relay::new(&args.data, SharedToken::new(args.token))?;
        let listener = tokio::net::TcpListener::bind(args.bind)
            .await
            .map_err(kiln_collab_server::ServeError::Io)?;
        eprintln!(
            "kiln-collab-server {VERSION} on {} (data in {})",
            listener.local_addr().unwrap_or(args.bind),
            args.data.display()
        );
        relay.serve(listener, stop_signal()).await
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kiln-collab-server: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn flags_parse_and_a_token_is_required() {
        let a = parse(args(&[
            "--bind",
            "0.0.0.0:9000",
            "--data",
            "/srv/k",
            "--token",
            "s3",
        ]));
        let a = a.ok().flatten();
        assert!(
            a.as_ref()
                .is_some_and(|a| a.bind.port() == 9000 && a.token == "s3")
        );
        assert!(parse(args(&["--bind", "nowhere", "--token", "t"])).is_err());
        assert!(parse(args(&["--frobnicate"])).is_err());
        assert!(matches!(parse(args(&["--help"])), Ok(None)));
    }

    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(VERSION, kiln_common::VERSION);
    }
}

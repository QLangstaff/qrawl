use clap::{Parser, Subcommand, Args};
use crate::{ApiResponse, Domain, LocalFsStore, Policy};
use crate::api::{self, Components};

#[derive(Parser)]
#[command(name="qrawl", version, about="Policies + extraction (JSON only)")]
pub struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand)]
    Policy(PolicyCmd),
    /// Extract content from a URL (auto-detects known vs unknown)
    Extract(ExtractArgs),
}

#[derive(Subcommand)]
enum PolicyCmd {
    Create { domain: String },
    Read(ReadArgs),
    Update { domain: String }, // Policy JSON from stdin
    Delete(DeleteArgs),
}

#[derive(Args)]
struct ReadArgs { target: String /* <domain> | all */ }

#[derive(Args)]
struct DeleteArgs {
    target: String,                 // <domain> | all
    #[arg(long="yes")] yes: bool,
}

#[derive(Args)]
struct ExtractArgs {
    /// The URL to extract. The engine will use the known pipeline if a policy exists,
    /// otherwise it will run the unknown pipeline and create a policy automatically.
    url: String,
}

pub fn run() {
    let cli = Cli::parse();
    let store = LocalFsStore::new().unwrap();
    let components = Components::default(); // Step-2: swap real fetcher/scraper here

    match cli.cmd {
        Command::Policy(pc) => policy_cmd(&store, pc),
        Command::Extract(args) => {
            finish(api::extract_url_auto(&store, &args.url, &components));
        }
    }
}

fn policy_cmd(store: &LocalFsStore, pc: PolicyCmd) {
    use std::io::{self, Read};
    match pc {
        PolicyCmd::Create { domain } => finish(api::policy_create(store, Domain(domain))),
        PolicyCmd::Read(ReadArgs { target }) => {
            if target == "all" { finish(api::policy_list(store)); }
            else { finish(api::policy_read(store, &target)); }
        }
        PolicyCmd::Update { domain } => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).unwrap();
            let mut incoming: Policy = match serde_json::from_str(&buf) {
                Ok(p) => p,
                Err(e) => { return print_json(ApiResponse::<()>::err(format!("invalid policy json: {e}"))); }
            };
            if incoming.domain.0 != domain {
                return print_json(ApiResponse::<()>::err("policy.domain mismatch with target"));
            }
            crate::policy::touch_updated(&mut incoming);
            finish(api::policy_update(store, &incoming).map(|_| incoming));
        }
        PolicyCmd::Delete(DeleteArgs { target, yes }) => {
            if !yes { return print_json(ApiResponse::<()>::err("refusing to delete without --yes")); }
            finish(api::policy_delete(store, &target).map(|_| serde_json::json!({"deleted": target})));
        }
    }
}

fn finish<T: serde::Serialize>(res: crate::Result<T>) {
    match res {
        Ok(v) => print_json(ApiResponse::ok(v)),
        Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
    }
}
fn print_json<T: serde::Serialize>(val: T) {
    // pretty JSON output
    println!("{}", serde_json::to_string_pretty(&val).unwrap());
}

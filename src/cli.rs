use crate::api::{self, Components};
use crate::{ApiResponse, Domain, LocalFsStore, Policy};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Map, Value};

#[derive(Parser)]
#[command(name = "qrawl", version, about = "Policies + extraction (JSON only)")]
pub struct Cli {
    /// URL to extract content from
    url: Option<String>,

    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand)]
    Policy(PolicyCmd),
}

#[derive(Subcommand)]
enum PolicyCmd {
    /// AUTO: probe domain, infer a working policy, verify live, save it.
    Create { domain: String },

    /// Read one policy or all
    Read(ReadArgs),

    /// Delete a policy (or all) with confirmation
    Delete(DeleteArgs),
}

#[derive(Args)]
struct ReadArgs {
    target: String, /* <domain> | all */
}

#[derive(Args)]
struct DeleteArgs {
    target: String, // <domain> | all
    #[arg(long = "yes")]
    yes: bool,
}

#[derive(Args)]
struct ExtractArgs {
    /// The URL to extract. The engine will use the known pipeline if a policy exists,
    /// otherwise it will run the unknown pipeline and create a policy automatically.
    url: String,
}

/* ---------- helpers: presentation ---------- */

fn policy_keyed_value(p: &Policy) -> Value {
    let inner = json!({
        "config": {
            "fetch": p.fetch,
            "scrape": p.scrape
        },
        "performance_profile": p.performance_profile
    });
    let mut top = Map::new();
    top.insert(p.domain.0.clone(), inner);
    Value::Object(top)
}

fn print_json_value(v: Value) {
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
}

/* -------------------------------------------------------------------- */

pub fn run() {
    let cli = Cli::parse();
    let store = LocalFsStore::new().unwrap();
    let components = Components::default();

    match (cli.url, cli.cmd) {
        // Direct URL extraction
        (Some(url), None) => {
            finish(api::extract_url(&store, &url, &components));
        }
        // Policy subcommands
        (None, Some(Command::Policy(pc))) => {
            policy_cmd(&store, &components, pc);
        }
        // No arguments - show help
        (None, None) => {
            println!("Usage: qrawl <URL> or qrawl policy <COMMAND> or qrawl --help");
        }
        // Invalid combination
        (Some(_), Some(_)) => {
            println!("Cannot specify both a URL and a subcommand. Use qrawl --help for usage.");
        }
    }
}

fn policy_cmd(store: &LocalFsStore, components: &Components, pc: PolicyCmd) {
    match pc {
        // AUTO: infer + verify + save; refuses overwrite
        PolicyCmd::Create { domain } => {
            let d = Domain::from_raw(&domain);
            match api::create_policy(store, d, components) {
                Ok(saved) => print_json_value(policy_keyed_value(&saved)),
                Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
            }
        }

        // READ
        PolicyCmd::Read(ReadArgs { target }) => {
            if target == "all" {
                match api::list_domains(store) {
                    Ok(domains) => print_json_value(json!(domains)),
                    Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
                }
            } else {
                match api::read_policy(store, &target) {
                    Ok(Some(p)) => print_json_value(policy_keyed_value(&p)),
                    Ok(None) => print_json(ApiResponse::<()>::err(format!(
                        "no policy for domain {}",
                        target
                    ))),
                    Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
                }
            }
        }

        // DELETE
        PolicyCmd::Delete(DeleteArgs { target, yes }) => {
            if !yes {
                return print_json(ApiResponse::<()>::err("refusing to delete without --yes"));
            }
            finish(
                api::delete_policy(store, &target)
                    .map(|_| serde_json::json!({ "deleted": target })),
            );
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
    println!("{}", serde_json::to_string_pretty(&val).unwrap());
}

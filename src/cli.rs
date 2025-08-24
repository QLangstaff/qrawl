use crate::api::{self, Components};
use crate::{ApiResponse, Domain, LocalFsStore, PerformanceProfile, Policy};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::IsTerminal;
use std::io::{self, Read};
use std::path::PathBuf;

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

    /// MANUAL: update (or create-if-missing) but only if supplied config works (reads JSON)
    /// Provide JSON via --file or stdin in shape:
    /// { "<domain>": { "config": { "fetch": {...}, "scrape": {...} } } }
    Update {
        domain: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Delete a policy (or all) with confirmation
    Delete(DeleteArgs),

    /// Show per-domain health; add --verbose to include config
    Status(StatusArgs),
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
struct StatusArgs {
    /// Include config (crawl/scrape) for each domain
    #[arg(long)]
    verbose: bool,
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

fn policies_keyed_value(policies: &[Policy]) -> Value {
    let mut top = Map::new();
    for p in policies {
        top.insert(
            p.domain.0.clone(),
            json!({
                "config": {
                    "fetch": p.fetch,
                    "scrape": p.scrape
                },
                "performance_profile": p.performance_profile
            }),
        );
    }
    Value::Object(top)
}

fn print_json_value(v: Value) {
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
}

/* ---------- read manual JSON for `update` ---------- */

fn read_json_input(file: &Option<PathBuf>) -> Result<String, ApiResponse<()>> {
    if let Some(path) = file {
        return fs::read_to_string(path)
            .map_err(|e| ApiResponse::<()>::err(format!("failed to read --file: {e}")));
    }
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Err(ApiResponse::<()>::err(
            "no input provided. pass JSON via --file <path> or pipe it on stdin",
        ));
    }
    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .map_err(|e| ApiResponse::<()>::err(format!("failed to read stdin: {e}")))?;
    Ok(buf)
}

fn parse_policy_for_domain(domain_arg: &str, json_text: &str) -> Result<Policy, ApiResponse<()>> {
    let v: Value = serde_json::from_str(json_text)
        .map_err(|e| ApiResponse::<()>::err(format!("invalid json: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| ApiResponse::<()>::err("expected top-level object"))?;
    if obj.len() != 1 {
        return Err(ApiResponse::<()>::err(
            "expected {\"<domain>\": {...}} with a single domain key",
        ));
    }
    let (key, body) = obj.iter().next().unwrap();
    if Domain::from_raw(domain_arg).0 != Domain::from_raw(key).0 {
        return Err(ApiResponse::<()>::err("policy.domain mismatch with target"));
    }
    let cfg = body
        .get("config")
        .and_then(|c| c.as_object())
        .cloned()
        .ok_or_else(|| ApiResponse::<()>::err("missing .config object"))?;

    let fetch: crate::types::FetchConfig =
        serde_json::from_value(cfg.get("fetch").cloned().unwrap_or(Value::Null))
            .map_err(|e| ApiResponse::<()>::err(format!("invalid .config.fetch: {e}")))?;
    let scrape: crate::types::ScrapeConfig =
        serde_json::from_value(cfg.get("scrape").cloned().unwrap_or(Value::Null))
            .map_err(|e| ApiResponse::<()>::err(format!("invalid .config.scrape: {e}")))?;

    let timeout_ms = fetch.timeout_ms;
    let strategy = fetch.bot_evasion_strategy.clone();

    Ok(Policy {
        domain: Domain::from_raw(key),
        fetch,
        scrape,
        performance_profile: PerformanceProfile {
            optimal_timeout_ms: timeout_ms,
            working_strategy: strategy.clone(),
            avg_response_size_bytes: 0, // Unknown from manual policy
            strategies_tried: vec![strategy],
            strategies_failed: vec![],
            last_tested_at: chrono::Utc::now(),
            success_rate: 1.0, // Assume manual policy works
        },
    })
}

/* -------------------------------------------------------------------- */

pub fn run() {
    let cli = Cli::parse();
    let store = LocalFsStore::new().unwrap();
    let components = Components::default();

    match (cli.url, cli.cmd) {
        // Direct URL extraction
        (Some(url), None) => {
            finish(api::extract_url_auto(&store, &url, &components));
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
            match api::policy_create_auto(store, d, components) {
                Ok(saved) => print_json_value(policy_keyed_value(&saved)),
                Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
            }
        }

        // READ
        PolicyCmd::Read(ReadArgs { target }) => {
            if target == "all" {
                match api::policy_list(store) {
                    Ok(list) => print_json_value(policies_keyed_value(&list)),
                    Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
                }
            } else {
                match api::policy_read(store, &target) {
                    Ok(Some(p)) => print_json_value(policy_keyed_value(&p)),
                    Ok(None) => print_json(ApiResponse::<()>::err(format!(
                        "no policy for domain {}",
                        target
                    ))),
                    Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
                }
            }
        }

        // MANUAL: update/create-if-missing with supplied JSON if it verifies
        PolicyCmd::Update { domain, file } => {
            match read_json_input(&file).and_then(|txt| parse_policy_for_domain(&domain, &txt)) {
                Ok(pol) => match api::policy_update_checked(store, &pol, components) {
                    Ok(_) => print_json_value(policy_keyed_value(&pol)),
                    Err(e) => print_json(ApiResponse::<()>::err(e.to_string())),
                },
                Err(api_err) => print_json(api_err),
            }
        }

        // DELETE
        PolicyCmd::Delete(DeleteArgs { target, yes }) => {
            if !yes {
                return print_json(ApiResponse::<()>::err("refusing to delete without --yes"));
            }
            finish(
                api::policy_delete(store, &target)
                    .map(|_| serde_json::json!({ "deleted": target })),
            );
        }

        // STATUS
        PolicyCmd::Status(StatusArgs { verbose }) => {
            match api::policy_status_all(store, components, verbose) {
                Ok(map) => {
                    let mut top_obj = Map::new();
                    let mut any_fail = false;

                    for (domain, st) in map {
                        let mut obj = Map::new();

                        if verbose {
                            if let Some(cfg) = st.config {
                                obj.insert(
                                    "config".into(),
                                    json!({ "fetch": cfg.fetch, "scrape": cfg.scrape }),
                                );
                            }
                        }

                        if st.status != "pass" {
                            any_fail = true;
                            obj.insert("status".into(), Value::String(st.status));
                            if let Some(err) = st.error {
                                obj.insert("error".into(), Value::String(err));
                            }
                        }

                        top_obj.insert(domain, Value::Object(obj));
                    }

                    if any_fail {
                        top_obj.insert(
                            "detail".to_string(),
                            Value::String("one or more domains failed".into()),
                        );
                    }

                    print_json_value(Value::Object(top_obj));
                }
                Err(e) => {
                    let out = serde_json::json!({ "detail": e.to_string() });
                    print_json_value(out);
                }
            }
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

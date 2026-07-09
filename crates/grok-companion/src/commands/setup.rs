use serde_json::json;

use crate::error::Result;
use crate::grok::{get_grok_availability, get_grok_login_status, node_available};
use crate::render::{json_pretty, render_setup_report};
use crate::state::{get_config, set_config};
use crate::workspace::resolve_workspace_root;

pub struct SetupArgs {
    pub enable_review_gate: bool,
    pub disable_review_gate: bool,
    pub json: bool,
    pub cwd: std::path::PathBuf,
}

pub fn run(args: SetupArgs) -> Result<i32> {
    if args.enable_review_gate && args.disable_review_gate {
        return Err(crate::error::CompanionError::msg(
            "Pass only one of --enable-review-gate or --disable-review-gate.",
        ));
    }

    let workspace = resolve_workspace_root(&args.cwd);
    let mut actions: Vec<String> = Vec::new();

    if args.enable_review_gate {
        set_config(&workspace, Some(true))?;
        actions.push("Enabled stop review gate.".into());
    }
    if args.disable_review_gate {
        set_config(&workspace, Some(false))?;
        actions.push("Disabled stop review gate.".into());
    }

    let (node_ok, node_detail) = node_available(Some(&args.cwd));
    let grok = get_grok_availability(Some(&args.cwd));
    let auth = get_grok_login_status(Some(&args.cwd));
    let config = get_config(&workspace);

    let mut next_steps = Vec::new();
    if !grok.available {
        next_steps.push(
            "Install Grok Build and ensure `grok` is on your PATH (or set GROK_BIN).".into(),
        );
    }
    if grok.available && !auth.logged_in {
        next_steps.push(
            "Authenticate Grok (open `grok` interactively or configure API credentials).".into(),
        );
    }
    if !config.stop_review_gate {
        next_steps.push(
            "Optional: run `grok-companion setup --enable-review-gate` to require a Grok review before stop.".into(),
        );
    }

    // Node is optional for the Rust companion itself — readiness is Grok on PATH.
    let ready = grok.available;
    let _ = node_ok;

    if args.json {
        let payload = json!({
            "ready": ready,
            "runtime": "rust",
            "node": { "available": node_ok, "detail": node_detail },
            "grok": {
                "available": grok.available,
                "detail": grok.detail,
                "binary": grok.binary.display().to_string()
            },
            "auth": {
                "available": auth.available,
                "loggedIn": auth.logged_in,
                "detail": auth.detail
            },
            "config": {
                "stopReviewGate": config.stop_review_gate
            },
            "actionsTaken": actions,
            "nextSteps": next_steps,
            "workspaceRoot": workspace.display().to_string()
        });
        println!("{}", json_pretty(&payload));
    } else {
        print!(
            "{}",
            render_setup_report(
                ready,
                &node_detail,
                &grok.detail,
                auth.logged_in,
                &auth.detail,
                &config,
                &actions,
                &next_steps,
            )
        );
    }

    Ok(if ready { 0 } else { 1 })
}

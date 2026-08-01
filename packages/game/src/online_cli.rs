use std::{
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::multiplayer::{
    QuinnClientConnector, QuinnEndpointConfig, QuinnHostConnectionDescriptor, QuinnHostListener,
    local_online_degraded_soak_summary, local_online_smoke_summary, local_online_soak_summary,
    production_online_acceptance_summary, scripted_latency_loss_online_playtest_summary,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineCliAction {
    Help,
    LocalSmoke,
    LocalSoak {
        ticks: u32,
    },
    LocalSoakJson {
        ticks: u32,
    },
    LocalDegradedSoak {
        ticks: u32,
    },
    LocalDegradedSoakJson {
        ticks: u32,
    },
    LatencyLossPlaytest,
    ProductionAcceptance,
    ProductionAcceptanceJson,
    LanQaPlanJson {
        descriptor_path: PathBuf,
        host_bind_addr: SocketAddr,
        host_advertise_addr: SocketAddr,
        client_bind_addr: SocketAddr,
        ticks: u32,
    },
    LanQaChecklistMarkdown {
        descriptor_path: PathBuf,
        host_bind_addr: SocketAddr,
        host_advertise_addr: SocketAddr,
        client_bind_addr: SocketAddr,
        ticks: u32,
    },
    InspectDescriptorFile {
        path: PathBuf,
    },
    HostDescriptorJson,
    HostDescriptorFile {
        path: PathBuf,
    },
    HostDescriptorFileOnAddr {
        path: PathBuf,
        bind_addr: SocketAddr,
        advertise_addr: SocketAddr,
    },
    JoinDescriptorFile {
        path: PathBuf,
    },
    JoinDescriptorFileOnAddr {
        path: PathBuf,
        bind_addr: SocketAddr,
    },
    HostGameplayDescriptorFile {
        path: PathBuf,
        ticks: u32,
    },
    HostGameplayDescriptorFileOnAddr {
        path: PathBuf,
        bind_addr: SocketAddr,
        advertise_addr: SocketAddr,
        ticks: u32,
    },
    JoinGameplayDescriptorFile {
        path: PathBuf,
        ticks: u32,
    },
    JoinGameplayDescriptorFileOnAddr {
        path: PathBuf,
        bind_addr: SocketAddr,
        ticks: u32,
    },
}

#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "online CLI parser is a flat flag table kept contiguous for discoverability"
)]
pub fn parse_online_cli_action<I, S>(args: I) -> Option<OnlineCliAction>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "--online-help" => return Some(OnlineCliAction::Help),
            "--online-local-smoke" => return Some(OnlineCliAction::LocalSmoke),
            "--online-local-soak" => {
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LocalSoak { ticks });
            }
            "--online-local-soak-json" => {
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LocalSoakJson { ticks });
            }
            "--online-local-degraded-soak" => {
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LocalDegradedSoak { ticks });
            }
            "--online-local-degraded-soak-json" => {
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LocalDegradedSoakJson { ticks });
            }
            "--online-latency-loss-playtest" => {
                return Some(OnlineCliAction::LatencyLossPlaytest);
            }
            "--online-production-acceptance" => {
                return Some(OnlineCliAction::ProductionAcceptance);
            }
            "--online-production-acceptance-json" => {
                return Some(OnlineCliAction::ProductionAcceptanceJson);
            }
            "--online-lan-qa-plan-json" => {
                let descriptor_path = PathBuf::from(args.next()?.as_ref());
                let host_bind_addr = args.next()?.as_ref().parse().ok()?;
                let host_advertise_addr = args.next()?.as_ref().parse().ok()?;
                let client_bind_addr = args.next()?.as_ref().parse().ok()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LanQaPlanJson {
                    descriptor_path,
                    host_bind_addr,
                    host_advertise_addr,
                    client_bind_addr,
                    ticks,
                });
            }
            "--online-lan-qa-checklist-md" => {
                let descriptor_path = PathBuf::from(args.next()?.as_ref());
                let host_bind_addr = args.next()?.as_ref().parse().ok()?;
                let host_advertise_addr = args.next()?.as_ref().parse().ok()?;
                let client_bind_addr = args.next()?.as_ref().parse().ok()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::LanQaChecklistMarkdown {
                    descriptor_path,
                    host_bind_addr,
                    host_advertise_addr,
                    client_bind_addr,
                    ticks,
                });
            }
            "--online-host-descriptor-json" => return Some(OnlineCliAction::HostDescriptorJson),
            "--online-inspect-descriptor-file" => {
                let path = args.next()?;
                return Some(OnlineCliAction::InspectDescriptorFile {
                    path: PathBuf::from(path.as_ref()),
                });
            }
            "--online-host-descriptor-file" => {
                let path = args.next()?;
                return Some(OnlineCliAction::HostDescriptorFile {
                    path: PathBuf::from(path.as_ref()),
                });
            }
            "--online-host-descriptor-file-on-addr" => {
                let path = args.next()?;
                let bind_addr = args.next()?.as_ref().parse().ok()?;
                let advertise_addr = args.next()?.as_ref().parse().ok()?;
                return Some(OnlineCliAction::HostDescriptorFileOnAddr {
                    path: PathBuf::from(path.as_ref()),
                    bind_addr,
                    advertise_addr,
                });
            }
            "--online-join-descriptor-file" => {
                let path = args.next()?;
                return Some(OnlineCliAction::JoinDescriptorFile {
                    path: PathBuf::from(path.as_ref()),
                });
            }
            "--online-join-descriptor-file-on-addr" => {
                let path = args.next()?;
                let bind_addr = args.next()?.as_ref().parse().ok()?;
                return Some(OnlineCliAction::JoinDescriptorFileOnAddr {
                    path: PathBuf::from(path.as_ref()),
                    bind_addr,
                });
            }
            "--online-host-gameplay-descriptor-file" => {
                let path = args.next()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::HostGameplayDescriptorFile {
                    path: PathBuf::from(path.as_ref()),
                    ticks,
                });
            }
            "--online-host-gameplay-descriptor-file-on-addr" => {
                let path = args.next()?;
                let bind_addr = args.next()?.as_ref().parse().ok()?;
                let advertise_addr = args.next()?.as_ref().parse().ok()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::HostGameplayDescriptorFileOnAddr {
                    path: PathBuf::from(path.as_ref()),
                    bind_addr,
                    advertise_addr,
                    ticks,
                });
            }
            "--online-join-gameplay-descriptor-file" => {
                let path = args.next()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::JoinGameplayDescriptorFile {
                    path: PathBuf::from(path.as_ref()),
                    ticks,
                });
            }
            "--online-join-gameplay-descriptor-file-on-addr" => {
                let path = args.next()?;
                let bind_addr = args.next()?.as_ref().parse().ok()?;
                let ticks = args.next()?.as_ref().parse().ok()?;
                if ticks == 0 {
                    return None;
                }
                return Some(OnlineCliAction::JoinGameplayDescriptorFileOnAddr {
                    path: PathBuf::from(path.as_ref()),
                    bind_addr,
                    ticks,
                });
            }
            _ => {}
        }
    }
    None
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanQaCommandPlan {
    pub descriptor_path: PathBuf,
    pub host_bind_addr: SocketAddr,
    pub host_advertise_addr: SocketAddr,
    pub client_bind_addr: SocketAddr,
    pub ticks: u32,
    pub one_shot_host_command: Vec<String>,
    pub one_shot_join_command: Vec<String>,
    pub gameplay_host_command: Vec<String>,
    pub gameplay_join_command: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostDescriptorInspection {
    pub host_addr: SocketAddr,
    pub server_name: String,
    pub certificate_der_bytes: usize,
    pub has_certificate_material: bool,
}

impl From<QuinnHostConnectionDescriptor> for HostDescriptorInspection {
    fn from(descriptor: QuinnHostConnectionDescriptor) -> Self {
        Self {
            host_addr: descriptor.host_addr,
            server_name: descriptor.server_name,
            certificate_der_bytes: descriptor.certificate_der.len(),
            has_certificate_material: !descriptor.certificate_der.is_empty(),
        }
    }
}

#[must_use]
pub fn build_lan_qa_command_plan(
    descriptor_path: PathBuf,
    host_bind_addr: SocketAddr,
    host_advertise_addr: SocketAddr,
    client_bind_addr: SocketAddr,
    ticks: u32,
) -> Option<LanQaCommandPlan> {
    if ticks == 0 {
        return None;
    }
    let descriptor = descriptor_path.display().to_string();
    Some(LanQaCommandPlan {
        descriptor_path,
        host_bind_addr,
        host_advertise_addr,
        client_bind_addr,
        ticks,
        one_shot_host_command: vec![
            "drillgame".to_owned(),
            "--online-host-descriptor-file-on-addr".to_owned(),
            descriptor.clone(),
            host_bind_addr.to_string(),
            host_advertise_addr.to_string(),
        ],
        one_shot_join_command: vec![
            "drillgame".to_owned(),
            "--online-join-descriptor-file-on-addr".to_owned(),
            descriptor.clone(),
            client_bind_addr.to_string(),
        ],
        gameplay_host_command: vec![
            "drillgame".to_owned(),
            "--online-host-gameplay-descriptor-file-on-addr".to_owned(),
            descriptor.clone(),
            host_bind_addr.to_string(),
            host_advertise_addr.to_string(),
            ticks.to_string(),
        ],
        gameplay_join_command: vec![
            "drillgame".to_owned(),
            "--online-join-gameplay-descriptor-file-on-addr".to_owned(),
            descriptor,
            client_bind_addr.to_string(),
            ticks.to_string(),
        ],
    })
}

#[must_use]
pub fn build_lan_qa_checklist_markdown(plan: &LanQaCommandPlan) -> String {
    format!(
        "# Drillgame LAN Multiplayer QA Checklist\n\n\
         ## Setup\n\n\
         - Descriptor path: `{}`\n\
         - Host bind address: `{}`\n\
         - Host advertised address: `{}`\n\
         - Client bind address: `{}`\n\
         - Gameplay ticks: `{}`\n\n\
         ## One-shot descriptor exchange\n\n\
         Host machine:\n\n```bash\n{}\n```\n\n\
         Client machine after host prints readiness:\n\n```bash\n{}\n```\n\n\
         Evidence:\n\n\
         - [ ] Host printed descriptor readiness.\n\
         - [ ] Client completed command/snapshot/correction/reconnect exchange.\n\
         - [ ] Host exited successfully after reconnect exchange.\n\
         - [ ] Record one-shot result: PASS / FAIL.\n\
         - [ ] Paste host stdout/stderr path or screenshot: __________.\n\
         - [ ] Paste client stdout/stderr path or screenshot: __________.\n\
         - [ ] Note descriptor inspection output/version/certificate fingerprint: __________.\n\n\
         ## Multi-tick gameplay descriptor exchange\n\n\
         Host machine:\n\n```bash\n{}\n```\n\n\
         Client machine after host prints readiness:\n\n```bash\n{}\n```\n\n\
         Evidence:\n\n\
         - [ ] Host printed gameplay descriptor readiness.\n\
         - [ ] Client completed the requested gameplay tick exchange.\n\
         - [ ] Host reported `ran {} ticks`.\n\
         - [ ] Record gameplay result: PASS / FAIL.\n\
         - [ ] Record observed latency/jitter/desync symptoms: __________.\n\
         - [ ] Record movement/drilling/terrain/cargo observation notes: __________.\n\
         - [ ] Paste host gameplay log path or screenshot: __________.\n\
         - [ ] Paste client gameplay log path or screenshot: __________.\n\n\
         ## Notes\n\n\
         - [ ] Host firewall allows the advertised UDP port.\n\
         - [ ] Both machines are on the intended LAN/VPN.\n\
         - [ ] Record host OS/firewall/NAT details: __________.\n\
         - [ ] Record client OS/firewall/NAT details: __________.\n\
         - [ ] Record exact Drillgame commit/build identifier: __________.\n\
         - [ ] File implementation issues for every FAIL before marking multiplayer complete.\n\
         - [ ] Attach stdout/stderr logs or screenshots to the release QA record.\n",
        plan.descriptor_path.display(),
        plan.host_bind_addr,
        plan.host_advertise_addr,
        plan.client_bind_addr,
        plan.ticks,
        plan.one_shot_host_command.join(" "),
        plan.one_shot_join_command.join(" "),
        plan.gameplay_host_command.join(" "),
        plan.gameplay_join_command.join(" "),
        plan.ticks,
    )
}

#[must_use]
pub const fn online_cli_usage() -> &'static str {
    "Online multiplayer CLI actions:\n  --online-help\n  --online-local-smoke\n  --online-local-soak <ticks>\n  --online-local-soak-json <ticks>\n  --online-local-degraded-soak <ticks>\n  --online-local-degraded-soak-json <ticks>\n  --online-latency-loss-playtest\n  --online-production-acceptance\n  --online-production-acceptance-json\n  --online-lan-qa-plan-json <descriptor-path> <host-bind-addr:port> <host-advertise-addr:port> <client-bind-addr:port> <ticks>\n  --online-lan-qa-checklist-md <descriptor-path> <host-bind-addr:port> <host-advertise-addr:port> <client-bind-addr:port> <ticks>\n  --online-host-descriptor-json\n  --online-inspect-descriptor-file <path>\n  --online-host-descriptor-file <path>\n  --online-host-descriptor-file-on-addr <path> <bind-addr:port> <advertise-addr:port>\n  --online-join-descriptor-file <path>\n  --online-join-descriptor-file-on-addr <path> <bind-addr:port>\n  --online-host-gameplay-descriptor-file <path> <ticks>\n  --online-host-gameplay-descriptor-file-on-addr <path> <bind-addr:port> <advertise-addr:port> <ticks>\n  --online-join-gameplay-descriptor-file <path> <ticks>\n  --online-join-gameplay-descriptor-file-on-addr <path> <bind-addr:port> <ticks>"
}

/// Execute a one-shot online CLI action.
///
/// # Errors
///
/// Returns an error when the Tokio runtime cannot be created, Quinn setup fails, or smoke checks fail.
pub fn run_online_cli_action(action: OnlineCliAction) -> Result<String, String> {
    match action {
        OnlineCliAction::Help => Ok(online_cli_usage().to_owned()),
        OnlineCliAction::LocalSmoke => run_local_smoke_cli_action(),
        OnlineCliAction::LocalSoak { ticks } => run_local_soak_cli_action(ticks),
        OnlineCliAction::LocalSoakJson { ticks } => run_local_soak_json_cli_action(ticks),
        OnlineCliAction::LocalDegradedSoak { ticks } => run_local_degraded_soak_cli_action(ticks),
        OnlineCliAction::LocalDegradedSoakJson { ticks } => {
            run_local_degraded_soak_json_cli_action(ticks)
        }
        OnlineCliAction::LatencyLossPlaytest => run_latency_loss_playtest_cli_action(),
        OnlineCliAction::ProductionAcceptance => run_production_acceptance_cli_action(),
        OnlineCliAction::ProductionAcceptanceJson => run_production_acceptance_json_cli_action(),
        OnlineCliAction::LanQaPlanJson {
            descriptor_path,
            host_bind_addr,
            host_advertise_addr,
            client_bind_addr,
            ticks,
        } => run_lan_qa_plan_json_cli_action(
            descriptor_path,
            host_bind_addr,
            host_advertise_addr,
            client_bind_addr,
            ticks,
        ),
        OnlineCliAction::LanQaChecklistMarkdown {
            descriptor_path,
            host_bind_addr,
            host_advertise_addr,
            client_bind_addr,
            ticks,
        } => run_lan_qa_checklist_markdown_cli_action(
            descriptor_path,
            host_bind_addr,
            host_advertise_addr,
            client_bind_addr,
            ticks,
        ),
        OnlineCliAction::HostDescriptorJson => run_host_descriptor_json_cli_action(),
        OnlineCliAction::InspectDescriptorFile { path } => {
            run_inspect_descriptor_file_cli_action(&path)
        }
        OnlineCliAction::HostDescriptorFile { path } => run_host_descriptor_file_cli_action(path),
        OnlineCliAction::HostDescriptorFileOnAddr {
            path,
            bind_addr,
            advertise_addr,
        } => run_host_descriptor_file_on_addr_cli_action(path, bind_addr, advertise_addr),
        OnlineCliAction::JoinDescriptorFile { path } => run_join_descriptor_file_cli_action(path),
        OnlineCliAction::JoinDescriptorFileOnAddr { path, bind_addr } => {
            run_join_descriptor_file_on_addr_cli_action(path, bind_addr)
        }
        OnlineCliAction::HostGameplayDescriptorFile { path, ticks } => {
            run_host_gameplay_descriptor_file_cli_action(path, ticks)
        }
        OnlineCliAction::HostGameplayDescriptorFileOnAddr {
            path,
            bind_addr,
            advertise_addr,
            ticks,
        } => run_host_gameplay_descriptor_file_on_addr_cli_action(
            path,
            bind_addr,
            advertise_addr,
            ticks,
        ),
        OnlineCliAction::JoinGameplayDescriptorFile { path, ticks } => {
            run_join_gameplay_descriptor_file_cli_action(path, ticks)
        }
        OnlineCliAction::JoinGameplayDescriptorFileOnAddr {
            path,
            bind_addr,
            ticks,
        } => run_join_gameplay_descriptor_file_on_addr_cli_action(path, bind_addr, ticks),
    }
}

fn build_current_thread_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
}

fn run_local_smoke_cli_action() -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(local_online_smoke_summary())
        .map_err(format_debug_error)?;
    if summary.passed() {
        Ok("local online smoke passed".to_owned())
    } else {
        Err("local online smoke did not satisfy all readiness checks".to_owned())
    }
}

fn run_local_soak_cli_action(ticks: u32) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(local_online_soak_summary(ticks))
        .map_err(format_debug_error)?;
    if summary.passed() {
        Ok(format!(
            "local online soak passed: ticks={}, commands={}, snapshots={}, deltas={}, chunks={}, corrections={}",
            summary.ticks_completed,
            summary.commands_exchanged,
            summary.snapshots_replicated,
            summary.deltas_replicated,
            summary.terrain_chunks_exchanged,
            summary.corrections_replicated
        ))
    } else {
        Err(format!(
            "local online soak failed: completed {}/{} ticks",
            summary.ticks_completed, summary.ticks_requested
        ))
    }
}

fn run_local_soak_json_cli_action(ticks: u32) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(local_online_soak_summary(ticks))
        .map_err(format_debug_error)?;
    serde_json::to_string_pretty(&summary).map_err(format_debug_error)
}

fn run_local_degraded_soak_cli_action(ticks: u32) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(local_online_degraded_soak_summary(ticks))
        .map_err(format_debug_error)?;
    if summary.passed() {
        Ok(format!(
            "local online degraded soak passed: ticks={}, degraded_coverage={}",
            summary.real_quinn_soak.ticks_completed,
            summary.degraded_network.covered.len()
        ))
    } else {
        Err(format!(
            "local online degraded soak failed: completed {}/{} ticks, degraded_coverage={}",
            summary.real_quinn_soak.ticks_completed,
            summary.real_quinn_soak.ticks_requested,
            summary.degraded_network.covered.len()
        ))
    }
}

fn run_local_degraded_soak_json_cli_action(ticks: u32) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(local_online_degraded_soak_summary(ticks))
        .map_err(format_debug_error)?;
    serde_json::to_string_pretty(&summary).map_err(format_debug_error)
}

fn run_latency_loss_playtest_cli_action() -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(scripted_latency_loss_online_playtest_summary())
        .map_err(format_debug_error)?;
    if summary.passed() {
        Ok("scripted latency/loss online playtest passed".to_owned())
    } else {
        Err("scripted latency/loss online playtest did not satisfy all readiness checks".to_owned())
    }
}

fn run_production_acceptance_cli_action() -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(production_online_acceptance_summary())
        .map_err(format_debug_error)?;
    if summary.direct_connect_mvp_passed() {
        Ok("production online direct-connect acceptance passed".to_owned())
    } else {
        Err(
            "production online direct-connect acceptance did not satisfy all readiness checks"
                .to_owned(),
        )
    }
}

fn run_production_acceptance_json_cli_action() -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let summary = runtime
        .block_on(production_online_acceptance_summary())
        .map_err(format_debug_error)?;
    serde_json::to_string_pretty(&summary).map_err(format_debug_error)
}

fn run_lan_qa_plan_json_cli_action(
    descriptor_path: PathBuf,
    host_bind_addr: SocketAddr,
    host_advertise_addr: SocketAddr,
    client_bind_addr: SocketAddr,
    ticks: u32,
) -> Result<String, String> {
    let plan = build_lan_qa_command_plan(
        descriptor_path,
        host_bind_addr,
        host_advertise_addr,
        client_bind_addr,
        ticks,
    )
    .ok_or_else(|| "LAN QA tick count must be greater than zero".to_owned())?;
    serde_json::to_string_pretty(&plan).map_err(format_debug_error)
}

fn run_lan_qa_checklist_markdown_cli_action(
    descriptor_path: PathBuf,
    host_bind_addr: SocketAddr,
    host_advertise_addr: SocketAddr,
    client_bind_addr: SocketAddr,
    ticks: u32,
) -> Result<String, String> {
    let plan = build_lan_qa_command_plan(
        descriptor_path,
        host_bind_addr,
        host_advertise_addr,
        client_bind_addr,
        ticks,
    )
    .ok_or_else(|| "LAN QA tick count must be greater than zero".to_owned())?;
    Ok(build_lan_qa_checklist_markdown(&plan))
}

fn run_host_descriptor_json_cli_action() -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    let _guard = runtime.enter();
    let listener = QuinnHostListener::bind_localhost(QuinnEndpointConfig::localhost_ephemeral())
        .map_err(format_debug_error)?;
    let descriptor = listener
        .connection_descriptor()
        .map_err(format_debug_error)?;
    serde_json::to_string(&descriptor).map_err(|error| error.to_string())
}

fn run_inspect_descriptor_file_cli_action(path: &Path) -> Result<String, String> {
    let json = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let descriptor: QuinnHostConnectionDescriptor =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let inspection = HostDescriptorInspection::from(descriptor);
    serde_json::to_string_pretty(&inspection).map_err(format_debug_error)
}

fn run_host_descriptor_file_cli_action(path: PathBuf) -> Result<String, String> {
    run_host_descriptor_file_with_config(path, QuinnEndpointConfig::localhost_ephemeral(), None)
}

fn run_host_descriptor_file_on_addr_cli_action(
    path: PathBuf,
    bind_addr: SocketAddr,
    advertise_addr: SocketAddr,
) -> Result<String, String> {
    run_host_descriptor_file_with_config(
        path,
        QuinnEndpointConfig { bind_addr },
        Some(advertise_addr),
    )
}

fn run_host_descriptor_file_with_config(
    path: PathBuf,
    config: QuinnEndpointConfig,
    advertise_addr: Option<SocketAddr>,
) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    runtime.block_on(async move {
        let listener = QuinnHostListener::bind_localhost(config).map_err(format_debug_error)?;
        let mut descriptor = listener
            .connection_descriptor()
            .map_err(format_debug_error)?;
        if let Some(addr) = advertise_addr {
            descriptor.host_addr = addr;
        }
        let json = serde_json::to_string(&descriptor).map_err(|error| error.to_string())?;
        std::fs::write(&path, json).map_err(|error| error.to_string())?;
        println!("online host descriptor ready");
        std::io::stdout()
            .flush()
            .map_err(|error| error.to_string())?;
        let packet_io = tokio::time::timeout(Duration::from_secs(5), listener.accept_packet_io())
            .await
            .map_err(|_| "timed out waiting for descriptor-file client".to_owned())?
            .map_err(format_debug_error)?;
        run_host_descriptor_file_packet_exchange(&packet_io).await?;
        let reconnect_io =
            tokio::time::timeout(Duration::from_secs(5), listener.accept_packet_io())
                .await
                .map_err(|_| "timed out waiting for descriptor-file reconnect".to_owned())?
                .map_err(format_debug_error)?;
        run_host_descriptor_file_reconnect_exchange(&reconnect_io).await?;
        Ok("host descriptor file exchanged command/snapshot/correction/reconnect".to_owned())
    })
}

fn run_join_descriptor_file_cli_action(path: PathBuf) -> Result<String, String> {
    run_join_descriptor_file_with_config(path, QuinnEndpointConfig::localhost_ephemeral())
}

fn run_join_descriptor_file_on_addr_cli_action(
    path: PathBuf,
    bind_addr: SocketAddr,
) -> Result<String, String> {
    run_join_descriptor_file_with_config(path, QuinnEndpointConfig { bind_addr })
}

fn run_join_descriptor_file_with_config(
    path: PathBuf,
    config: QuinnEndpointConfig,
) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    runtime.block_on(async move {
        let json = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let descriptor: QuinnHostConnectionDescriptor =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        let connector = QuinnClientConnector::bind_from_host_descriptor(config, &descriptor)
            .map_err(format_debug_error)?;
        let packet_io = connector
            .connect_packet_io(descriptor.host_addr, &descriptor.server_name)
            .await
            .map_err(format_debug_error)?;
        run_join_descriptor_file_packet_exchange(&packet_io).await?;
        let reconnect_io = connector
            .connect_packet_io(descriptor.host_addr, &descriptor.server_name)
            .await
            .map_err(format_debug_error)?;
        run_join_descriptor_file_reconnect_exchange(&reconnect_io).await?;
        Ok("joined descriptor host and exchanged command/snapshot/correction/reconnect".to_owned())
    })
}

fn run_host_gameplay_descriptor_file_cli_action(
    path: PathBuf,
    ticks: u32,
) -> Result<String, String> {
    run_host_gameplay_descriptor_file_with_config(
        path,
        QuinnEndpointConfig::localhost_ephemeral(),
        None,
        ticks,
    )
}

fn run_host_gameplay_descriptor_file_on_addr_cli_action(
    path: PathBuf,
    bind_addr: SocketAddr,
    advertise_addr: SocketAddr,
    ticks: u32,
) -> Result<String, String> {
    run_host_gameplay_descriptor_file_with_config(
        path,
        QuinnEndpointConfig { bind_addr },
        Some(advertise_addr),
        ticks,
    )
}

fn run_host_gameplay_descriptor_file_with_config(
    path: PathBuf,
    config: QuinnEndpointConfig,
    advertise_addr: Option<SocketAddr>,
    ticks: u32,
) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    runtime.block_on(async move {
        let listener = QuinnHostListener::bind_localhost(config).map_err(format_debug_error)?;
        let mut descriptor = listener
            .connection_descriptor()
            .map_err(format_debug_error)?;
        if let Some(addr) = advertise_addr {
            descriptor.host_addr = addr;
        }
        let json = serde_json::to_string(&descriptor).map_err(|error| error.to_string())?;
        std::fs::write(&path, json).map_err(|error| error.to_string())?;
        println!("online gameplay host descriptor ready");
        std::io::stdout()
            .flush()
            .map_err(|error| error.to_string())?;
        let packet_io = tokio::time::timeout(Duration::from_secs(5), listener.accept_packet_io())
            .await
            .map_err(|_| "timed out waiting for gameplay descriptor-file client".to_owned())?
            .map_err(format_debug_error)?;
        run_host_gameplay_descriptor_ticks(&packet_io, ticks).await?;
        Ok(format!("host gameplay descriptor file ran {ticks} ticks"))
    })
}

fn run_join_gameplay_descriptor_file_cli_action(
    path: PathBuf,
    ticks: u32,
) -> Result<String, String> {
    run_join_gameplay_descriptor_file_with_config(
        path,
        QuinnEndpointConfig::localhost_ephemeral(),
        ticks,
    )
}

fn run_join_gameplay_descriptor_file_on_addr_cli_action(
    path: PathBuf,
    bind_addr: SocketAddr,
    ticks: u32,
) -> Result<String, String> {
    run_join_gameplay_descriptor_file_with_config(path, QuinnEndpointConfig { bind_addr }, ticks)
}

fn run_join_gameplay_descriptor_file_with_config(
    path: PathBuf,
    config: QuinnEndpointConfig,
    ticks: u32,
) -> Result<String, String> {
    let runtime = build_current_thread_runtime()?;
    runtime.block_on(async move {
        let json = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let descriptor: QuinnHostConnectionDescriptor =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        let connector = QuinnClientConnector::bind_from_host_descriptor(config, &descriptor)
            .map_err(format_debug_error)?;
        let packet_io = connector
            .connect_packet_io(descriptor.host_addr, &descriptor.server_name)
            .await
            .map_err(format_debug_error)?;
        run_join_gameplay_descriptor_ticks(&packet_io, ticks).await?;
        Ok(format!("joined gameplay descriptor host for {ticks} ticks"))
    })
}

async fn run_host_gameplay_descriptor_ticks(
    packet_io: &crate::multiplayer::QuinnPacketIo,
    ticks: u32,
) -> Result<(), String> {
    for tick_index in 0..ticks {
        let command_packet =
            tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
                .await
                .map_err(|_| "timed out waiting for gameplay command packet".to_owned())?
                .map_err(format_debug_error)?;
        let crate::multiplayer::ProtocolMessage::CommandPacket(packet) = command_packet.message
        else {
            return Err("gameplay host expected command packet".to_owned());
        };
        if packet.commands.len() != 1 {
            return Err("gameplay host expected one command per tick".to_owned());
        }
        packet_io
            .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
                crate::multiplayer::ProtocolMessage::SnapshotKeyframe {
                    snapshot: gameplay_descriptor_snapshot(tick_index),
                },
            ))
            .await
            .map_err(format_debug_error)?;
    }
    tokio::task::yield_now().await;
    packet_io.close(b"gameplay descriptor exchange complete");
    Ok(())
}

async fn run_join_gameplay_descriptor_ticks(
    packet_io: &crate::multiplayer::QuinnPacketIo,
    ticks: u32,
) -> Result<(), String> {
    for tick_index in 0..ticks {
        packet_io
            .send_packet(gameplay_descriptor_command_packet(tick_index))
            .await
            .map_err(format_debug_error)?;
        let snapshot =
            tokio::time::timeout(Duration::from_secs(5), packet_io.receive_datagram_packet())
                .await
                .map_err(|_| "timed out waiting for gameplay snapshot".to_owned())?
                .map_err(format_debug_error)?;
        let crate::multiplayer::ProtocolMessage::SnapshotKeyframe { snapshot } = snapshot.message
        else {
            return Err("gameplay join expected snapshot".to_owned());
        };
        if snapshot.tick != crate::multiplayer::SimulationTick::new(u64::from(tick_index) + 10) {
            return Err("gameplay join received unexpected snapshot tick".to_owned());
        }
    }
    tokio::task::yield_now().await;
    Ok(())
}

async fn run_host_descriptor_file_packet_exchange(
    packet_io: &crate::multiplayer::QuinnPacketIo,
) -> Result<(), String> {
    let terrain_request =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file terrain request".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
        chunk_x,
        chunk_y,
        known_revision: _,
    } = terrain_request.message
    else {
        return Err("descriptor-file host expected terrain request".to_owned());
    };
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::TerrainChunkResponse {
                chunk_x,
                chunk_y,
                revision: 1,
                tiles: Vec::new(),
            },
        ))
        .await
        .map_err(format_debug_error)?;
    let command_packet =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file command packet".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::CommandPacket(_) = command_packet.message else {
        return Err("descriptor-file host expected command packet".to_owned());
    };
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::SnapshotKeyframe {
                snapshot: descriptor_file_snapshot(),
            },
        ))
        .await
        .map_err(format_debug_error)?;
    let snapshot_ack =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file snapshot ack".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
        chunk_x: 99,
        chunk_y: 99,
        known_revision: 1,
    } = snapshot_ack.message
    else {
        return Err("descriptor-file host expected snapshot ack".to_owned());
    };
    packet_io.close(b"descriptor-file exchange complete");
    Ok(())
}

async fn run_host_descriptor_file_reconnect_exchange(
    packet_io: &crate::multiplayer::QuinnPacketIo,
) -> Result<(), String> {
    let reconnect =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file reconnect request".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::ReconnectRequest {
        client_id,
        session_token,
    } = reconnect.message
    else {
        return Err("descriptor-file host expected reconnect request".to_owned());
    };
    if session_token != descriptor_file_session_token() {
        return Err("descriptor-file host received wrong reconnect token".to_owned());
    }
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::JoinAccepted {
                client_id,
                player_id: crate::multiplayer::PlayerId::new(1),
                snapshot_tick: crate::multiplayer::SimulationTick::new(3),
            },
        ))
        .await
        .map_err(format_debug_error)?;
    tokio::task::yield_now().await;
    packet_io.close(b"descriptor-file reconnect complete");
    Ok(())
}

async fn run_join_descriptor_file_packet_exchange(
    packet_io: &crate::multiplayer::QuinnPacketIo,
) -> Result<(), String> {
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
                chunk_x: 1,
                chunk_y: 2,
                known_revision: 0,
            },
        ))
        .await
        .map_err(format_debug_error)?;
    let terrain_response =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file terrain response".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::TerrainChunkResponse { revision: 1, .. } =
        terrain_response.message
    else {
        return Err("descriptor-file join expected terrain response".to_owned());
    };
    packet_io
        .send_packet(descriptor_file_command_packet())
        .await
        .map_err(format_debug_error)?;
    let snapshot =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_datagram_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file snapshot".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::SnapshotKeyframe { snapshot } = snapshot.message
    else {
        return Err("descriptor-file join expected snapshot".to_owned());
    };
    validate_descriptor_file_correction(&snapshot)?;
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::TerrainChunkRequest {
                chunk_x: 99,
                chunk_y: 99,
                known_revision: 1,
            },
        ))
        .await
        .map_err(format_debug_error)?;
    tokio::task::yield_now().await;
    Ok(())
}

fn validate_descriptor_file_correction(
    snapshot: &crate::multiplayer::NetworkWorldSnapshot,
) -> Result<(), String> {
    let Some(authoritative) = snapshot.players.first() else {
        return Err("descriptor-file snapshot was empty".to_owned());
    };
    let authoritative_snapshot = crate::session::PlayerSnapshot {
        player_id: authoritative.player_id,
        x: authoritative.x,
        y: authoritative.y,
        velocity_x: authoritative.velocity_x,
        velocity_y: authoritative.velocity_y,
        fuel: authoritative.fuel,
        fuel_capacity: authoritative.fuel.max(100.0),
        hull: authoritative.hull,
        max_hull: authoritative.hull.max(100.0),
        credits: authoritative.credits,
        cargo_used: authoritative.cargo_used,
        cargo: authoritative.cargo.clone(),
        artifacts: authoritative.artifacts.clone(),
        materials: authoritative.materials.clone(),
        loadout: authoritative.loadout.clone(),
        scanner_cooldown_seconds: authoritative.scanner_cooldown_seconds,
    };
    let predicted = crate::session::PredictedMovement {
        player_id: authoritative.player_id,
        x: authoritative.x + 24.0,
        y: authoritative.y,
        velocity_x: authoritative.velocity_x,
        velocity_y: authoritative.velocity_y,
    };
    let reconciled = crate::session::ClientPredictionState::reconcile_movement(
        predicted,
        &authoritative_snapshot,
    );
    if reconciled.correction_plan != crate::session::CorrectionPlan::Snap {
        return Err("descriptor-file correction did not request snap".to_owned());
    }
    let presentation =
        crate::session::CorrectionPresentationFrame::from_reconciliation(&reconciled, 0.5);
    if !presentation.snap_applied {
        return Err("descriptor-file correction snap was not applied".to_owned());
    }
    Ok(())
}

async fn run_join_descriptor_file_reconnect_exchange(
    packet_io: &crate::multiplayer::QuinnPacketIo,
) -> Result<(), String> {
    packet_io
        .send_packet(crate::multiplayer::VersionedProtocolPacket::new(
            crate::multiplayer::ProtocolMessage::ReconnectRequest {
                client_id: crate::multiplayer::ClientId::new(1),
                session_token: descriptor_file_session_token(),
            },
        ))
        .await
        .map_err(format_debug_error)?;
    let accepted =
        tokio::time::timeout(Duration::from_secs(5), packet_io.receive_reliable_packet())
            .await
            .map_err(|_| "timed out waiting for descriptor-file reconnect accepted".to_owned())?
            .map_err(format_debug_error)?;
    let crate::multiplayer::ProtocolMessage::JoinAccepted {
        client_id,
        player_id,
        snapshot_tick,
    } = accepted.message
    else {
        return Err("descriptor-file join expected reconnect acceptance".to_owned());
    };
    if client_id != crate::multiplayer::ClientId::new(1)
        || player_id != crate::multiplayer::PlayerId::new(1)
        || snapshot_tick != crate::multiplayer::SimulationTick::new(3)
    {
        return Err("descriptor-file reconnect acceptance carried unexpected identity".to_owned());
    }
    Ok(())
}

fn format_debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

const fn descriptor_file_session_token() -> crate::multiplayer::SessionToken {
    crate::multiplayer::SessionToken::new(0xD411_600D_0000_0001)
}

fn gameplay_descriptor_command_packet(
    tick_index: u32,
) -> crate::multiplayer::VersionedProtocolPacket {
    crate::multiplayer::VersionedProtocolPacket::new(
        crate::multiplayer::ProtocolMessage::CommandPacket(crate::multiplayer::CommandPacket {
            client_id: crate::multiplayer::ClientId::new(1),
            commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: crate::multiplayer::PlayerId::new(1),
                sequence: crate::multiplayer::InputSequence::new(tick_index + 1),
                target_tick: crate::multiplayer::SimulationTick::new(u64::from(tick_index) + 10),
                command: crate::multiplayer::PlayerCommand::Movement {
                    horizontal: if tick_index.is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    },
                    thrust: tick_index.is_multiple_of(2),
                    drill_down: !tick_index.is_multiple_of(2),
                },
            }],
        }),
    )
}

fn gameplay_descriptor_snapshot(tick_index: u32) -> crate::multiplayer::NetworkWorldSnapshot {
    let tick_offset = f32::from(u16::try_from(tick_index).unwrap_or(u16::MAX));
    crate::multiplayer::NetworkWorldSnapshot {
        tick: crate::multiplayer::SimulationTick::new(u64::from(tick_index) + 10),
        players: vec![crate::multiplayer::NetworkPlayerSnapshot {
            player_id: crate::multiplayer::PlayerId::new(1),
            x: 10.0 + tick_offset,
            y: 20.0 + tick_offset,
            velocity_x: 1.0,
            velocity_y: 0.0,
            fuel: 99.0 - tick_offset,
            hull: 100.0,
            credits: 5 + tick_index,
            cargo_used: tick_index,
            cargo: std::collections::BTreeMap::new(),
            artifacts: std::collections::BTreeMap::new(),
            materials: std::collections::BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        }],
    }
}

fn descriptor_file_command_packet() -> crate::multiplayer::VersionedProtocolPacket {
    crate::multiplayer::VersionedProtocolPacket::new(
        crate::multiplayer::ProtocolMessage::CommandPacket(crate::multiplayer::CommandPacket {
            client_id: crate::multiplayer::ClientId::new(1),
            commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: crate::multiplayer::PlayerId::new(1),
                sequence: crate::multiplayer::InputSequence::new(1),
                target_tick: crate::multiplayer::SimulationTick::new(1),
                command: crate::multiplayer::PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: true,
                    drill_down: false,
                },
            }],
        }),
    )
}

fn descriptor_file_snapshot() -> crate::multiplayer::NetworkWorldSnapshot {
    crate::multiplayer::NetworkWorldSnapshot {
        tick: crate::multiplayer::SimulationTick::new(2),
        players: vec![crate::multiplayer::NetworkPlayerSnapshot {
            player_id: crate::multiplayer::PlayerId::new(1),
            x: 10.0,
            y: 20.0,
            velocity_x: 1.0,
            velocity_y: 0.0,
            fuel: 99.0,
            hull: 100.0,
            credits: 5,
            cargo_used: 0,
            cargo: std::collections::BTreeMap::new(),
            artifacts: std::collections::BTreeMap::new(),
            materials: std::collections::BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplayer::QuinnHostConnectionDescriptor;

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "parser table coverage is easiest to audit in one contiguous test"
    )]
    fn online_cli_parser_recognizes_local_smoke_and_descriptor_actions() {
        assert_eq!(
            parse_online_cli_action(["--online-help"]),
            Some(OnlineCliAction::Help)
        );
        assert_eq!(
            parse_online_cli_action(["--online-local-smoke"]),
            Some(OnlineCliAction::LocalSmoke)
        );
        assert_eq!(
            parse_online_cli_action(["--online-local-soak", "5"]),
            Some(OnlineCliAction::LocalSoak { ticks: 5 })
        );
        assert_eq!(
            parse_online_cli_action(["--online-local-soak-json", "5"]),
            Some(OnlineCliAction::LocalSoakJson { ticks: 5 })
        );
        assert_eq!(
            parse_online_cli_action(["--online-local-degraded-soak", "5"]),
            Some(OnlineCliAction::LocalDegradedSoak { ticks: 5 })
        );
        assert_eq!(
            parse_online_cli_action(["--online-local-degraded-soak-json", "5"]),
            Some(OnlineCliAction::LocalDegradedSoakJson { ticks: 5 })
        );
        assert_eq!(
            parse_online_cli_action(["--online-latency-loss-playtest"]),
            Some(OnlineCliAction::LatencyLossPlaytest)
        );
        assert_eq!(
            parse_online_cli_action(["--online-production-acceptance"]),
            Some(OnlineCliAction::ProductionAcceptance)
        );
        assert_eq!(
            parse_online_cli_action(["--online-production-acceptance-json"]),
            Some(OnlineCliAction::ProductionAcceptanceJson)
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-lan-qa-plan-json",
                "/tmp/drillgame-host.json",
                "0.0.0.0:4242",
                "192.0.2.15:4242",
                "0.0.0.0:0",
                "60"
            ]),
            Some(OnlineCliAction::LanQaPlanJson {
                descriptor_path: PathBuf::from("/tmp/drillgame-host.json"),
                host_bind_addr: "0.0.0.0:4242".parse().expect("host bind parses"),
                host_advertise_addr: "192.0.2.15:4242".parse().expect("host advertise parses"),
                client_bind_addr: "0.0.0.0:0".parse().expect("client bind parses"),
                ticks: 60,
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-lan-qa-checklist-md",
                "/tmp/drillgame-host.md.json",
                "0.0.0.0:4242",
                "192.0.2.15:4242",
                "0.0.0.0:0",
                "60"
            ]),
            Some(OnlineCliAction::LanQaChecklistMarkdown {
                descriptor_path: PathBuf::from("/tmp/drillgame-host.md.json"),
                host_bind_addr: "0.0.0.0:4242".parse().expect("host bind parses"),
                host_advertise_addr: "192.0.2.15:4242".parse().expect("host advertise parses"),
                client_bind_addr: "0.0.0.0:0".parse().expect("client bind parses"),
                ticks: 60,
            })
        );
        assert_eq!(
            parse_online_cli_action(["--online-host-descriptor-json"]),
            Some(OnlineCliAction::HostDescriptorJson)
        );
        assert_eq!(
            parse_online_cli_action(["--online-inspect-descriptor-file", "/tmp/host.json"]),
            Some(OnlineCliAction::InspectDescriptorFile {
                path: PathBuf::from("/tmp/host.json")
            })
        );
        assert_eq!(
            parse_online_cli_action(["--online-host-descriptor-file", "/tmp/host.json"]),
            Some(OnlineCliAction::HostDescriptorFile {
                path: PathBuf::from("/tmp/host.json")
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-host-descriptor-file-on-addr",
                "/tmp/lan-host.json",
                "0.0.0.0:0",
                "192.0.2.10:4242"
            ]),
            Some(OnlineCliAction::HostDescriptorFileOnAddr {
                path: PathBuf::from("/tmp/lan-host.json"),
                bind_addr: "0.0.0.0:0".parse().expect("bind addr parses"),
                advertise_addr: "192.0.2.10:4242".parse().expect("advertise addr parses"),
            })
        );
        assert_eq!(
            parse_online_cli_action(["--online-join-descriptor-file", "/tmp/host.json"]),
            Some(OnlineCliAction::JoinDescriptorFile {
                path: PathBuf::from("/tmp/host.json")
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-join-descriptor-file-on-addr",
                "/tmp/host.json",
                "0.0.0.0:0"
            ]),
            Some(OnlineCliAction::JoinDescriptorFileOnAddr {
                path: PathBuf::from("/tmp/host.json"),
                bind_addr: "0.0.0.0:0".parse().expect("bind addr parses"),
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-host-gameplay-descriptor-file",
                "/tmp/gameplay-host.json",
                "3"
            ]),
            Some(OnlineCliAction::HostGameplayDescriptorFile {
                path: PathBuf::from("/tmp/gameplay-host.json"),
                ticks: 3,
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-host-gameplay-descriptor-file-on-addr",
                "/tmp/gameplay-lan-host.json",
                "0.0.0.0:0",
                "192.0.2.11:5252",
                "4"
            ]),
            Some(OnlineCliAction::HostGameplayDescriptorFileOnAddr {
                path: PathBuf::from("/tmp/gameplay-lan-host.json"),
                bind_addr: "0.0.0.0:0".parse().expect("bind addr parses"),
                advertise_addr: "192.0.2.11:5252".parse().expect("advertise addr parses"),
                ticks: 4,
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-join-gameplay-descriptor-file",
                "/tmp/gameplay-host.json",
                "3"
            ]),
            Some(OnlineCliAction::JoinGameplayDescriptorFile {
                path: PathBuf::from("/tmp/gameplay-host.json"),
                ticks: 3,
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-join-gameplay-descriptor-file-on-addr",
                "/tmp/gameplay-host.json",
                "0.0.0.0:0",
                "3"
            ]),
            Some(OnlineCliAction::JoinGameplayDescriptorFileOnAddr {
                path: PathBuf::from("/tmp/gameplay-host.json"),
                bind_addr: "0.0.0.0:0".parse().expect("bind addr parses"),
                ticks: 3,
            })
        );
        assert_eq!(
            parse_online_cli_action([
                "--online-join-gameplay-descriptor-file",
                "/tmp/gameplay-host.json",
                "0"
            ]),
            None
        );
        assert_eq!(
            parse_online_cli_action(vec![
                "--online-host-gameplay-descriptor-file".to_owned(),
                "/tmp/owned-gameplay-host.json".to_owned(),
                "2".to_owned(),
            ]),
            Some(OnlineCliAction::HostGameplayDescriptorFile {
                path: PathBuf::from("/tmp/owned-gameplay-host.json"),
                ticks: 2,
            })
        );
        assert_eq!(parse_online_cli_action(["--fullscreen"]), None);
    }

    #[test]
    fn lan_qa_command_plan_lists_host_and_join_commands() {
        let plan = build_lan_qa_command_plan(
            PathBuf::from("/tmp/drillgame-host.json"),
            "0.0.0.0:4242".parse().expect("host bind parses"),
            "192.0.2.15:4242".parse().expect("host advertise parses"),
            "0.0.0.0:0".parse().expect("client bind parses"),
            60,
        )
        .expect("non-zero ticks build a plan");

        assert!(
            plan.one_shot_host_command
                .contains(&"--online-host-descriptor-file-on-addr".to_owned())
        );
        assert!(
            plan.one_shot_join_command
                .contains(&"--online-join-descriptor-file-on-addr".to_owned())
        );
        assert!(
            plan.gameplay_host_command
                .contains(&"--online-host-gameplay-descriptor-file-on-addr".to_owned())
        );
        assert!(
            plan.gameplay_join_command
                .contains(&"--online-join-gameplay-descriptor-file-on-addr".to_owned())
        );
        assert_eq!(plan.ticks, 60);
        let checklist = build_lan_qa_checklist_markdown(&plan);
        assert!(checklist.contains("Drillgame LAN Multiplayer QA Checklist"));
        assert!(checklist.contains("--online-host-gameplay-descriptor-file-on-addr"));
        assert!(checklist.contains("--online-join-gameplay-descriptor-file-on-addr"));
        assert!(checklist.contains("Record one-shot result: PASS / FAIL"));
        assert!(checklist.contains("Record gameplay result: PASS / FAIL"));
        assert!(checklist.contains("host OS/firewall/NAT details"));
        assert!(checklist.contains("movement/drilling/terrain/cargo observation notes"));
    }

    #[test]
    fn descriptor_inspection_reports_address_and_certificate_size() {
        let inspection = HostDescriptorInspection::from(QuinnHostConnectionDescriptor {
            host_addr: "127.0.0.1:4242".parse().expect("host addr parses"),
            server_name: "localhost".to_owned(),
            certificate_der: vec![1, 2, 3],
        });

        assert_eq!(inspection.host_addr.to_string(), "127.0.0.1:4242");
        assert_eq!(inspection.server_name, "localhost");
        assert_eq!(inspection.certificate_der_bytes, 3);
        assert!(inspection.has_certificate_material);
    }

    #[test]
    fn online_cli_descriptor_action_emits_serialized_descriptor() {
        let json = run_online_cli_action(OnlineCliAction::HostDescriptorJson)
            .expect("descriptor json is emitted");
        let descriptor: QuinnHostConnectionDescriptor =
            serde_json::from_str(&json).expect("descriptor decodes");

        assert!(!descriptor.certificate_der.is_empty());
        assert!(!descriptor.server_name.is_empty());
    }
}

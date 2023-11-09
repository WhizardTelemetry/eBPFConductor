use std::{
    env,
    fs::{create_dir_all, File},
    io::{BufRead, BufReader},
    str::FromStr,
};

use anyhow::{bail, Context};
use bpflet_api::config::Config;
use log::info;
use nix::{
    libc::RLIM_INFINITY,
    sys::resource::{setrlimit, Resource},
};
use systemd_journal_logger::{connected_to_journal, JournalLog};

use crate::{
    cli::args::{SystemSubcommand},
    serve::serve,
    utils::{create_bpffs, set_dir_permissions},
    BPFLET_ENV_LOG_LEVEL,
};

impl SystemSubcommand {
    pub(crate) fn execute(&self, config: &Config) -> anyhow::Result<()> {
        match self {
            SystemSubcommand::Start => execute_service(config),
        }
    }
}

pub(crate) fn execute_service(config: &Config) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            if connected_to_journal() {
                // If bpflet is running as a service, log to journald.
                JournalLog::default()
                    .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))])
                    .install()
                    .unwrap();
                manage_journal_log_level();
                info!("Log using journald");
            } else {
                // Otherwise fall back to logging to standard error.
                env_logger::init();
                info!("Log using env_logger");
            }

            has_cap(caps::CapSet::Effective, caps::Capability::CAP_BPF);
            has_cap(caps::CapSet::Effective, caps::Capability::CAP_SYS_ADMIN);

            setrlimit(Resource::RLIMIT_MEMLOCK, RLIM_INFINITY, RLIM_INFINITY).unwrap();

            // Create directories associated with bpflet
            use bpflet_api::util::directories::*;
            create_dir_all(RTDIR).context("unable to create runtime directory")?;
            create_dir_all(RTDIR_FS).context("unable to create mountpoint")?;
            create_dir_all(RTDIR_TC_INGRESS_DISPATCHER)
                .context("unable to create dispatcher directory")?;
            create_dir_all(RTDIR_TC_EGRESS_DISPATCHER)
                .context("unable to create dispatcher directory")?;
            create_dir_all(RTDIR_XDP_DISPATCHER)
                .context("unable to create dispatcher directory")?;
            create_dir_all(RTDIR_PROGRAMS).context("unable to create programs directory")?;

            if !is_bpffs_mounted()? {
                create_bpffs(RTDIR_FS)?;
            }
            create_dir_all(RTDIR_FS_XDP).context("unable to create xdp distpacher directory")?;
            create_dir_all(RTDIR_FS_TC_INGRESS)
                .context("unable to create tc ingress dispatcher directory")?;
            create_dir_all(RTDIR_FS_TC_EGRESS)
                .context("unable to create tc egress dispatcher directory")?;
            create_dir_all(RTDIR_FS_MAPS).context("unable to create maps directory")?;
            create_dir_all(RTDIR_SOCK).context("unable to create socket directory")?;

            create_dir_all(STDIR).context("unable to create state directory")?;

            create_dir_all(CFGDIR_STATIC_PROGRAMS)
                .context("unable to create static programs directory")?;

            set_dir_permissions(CFGDIR, CFGDIR_MODE).await;
            set_dir_permissions(RTDIR, RTDIR_MODE).await;
            set_dir_permissions(STDIR, STDIR_MODE).await;

            serve(config, CFGDIR_STATIC_PROGRAMS).await?;
            Ok(())
        })
}

fn manage_journal_log_level() {
    // env_logger uses the environment variable RUST_LOG to set the log
    // level. Parse RUST_LOG to set the log level for journald.
    log::set_max_level(log::LevelFilter::Error);
    if env::var(BPFLET_ENV_LOG_LEVEL).is_ok() {
        let rust_log = log::LevelFilter::from_str(&env::var(BPFLET_ENV_LOG_LEVEL).unwrap());
        match rust_log {
            Ok(value) => log::set_max_level(value),
            Err(e) => log::error!("Invalid Log Level: {}", e),
        }
    }
}

fn has_cap(cset: caps::CapSet, cap: caps::Capability) {
    info!("Has {}: {}", cap, caps::has_cap(None, cset, cap).unwrap());
}

fn is_bpffs_mounted() -> Result<bool, anyhow::Error> {
    let file = File::open("/proc/mounts").context("Failed to open /proc/mounts")?;
    let reader = BufReader::new(file);
    for l in reader.lines() {
        match l {
            Ok(line) => {
                let parts: Vec<&str> = line.split(' ').collect();
                if parts.len() != 6 {
                    bail!("expected 6 parts in proc mount")
                }
                if parts[0] == "none" && parts[1].contains("bpflet") && parts[2] == "bpf" {
                    return Ok(true);
                }
            }
            Err(e) => bail!("problem reading lines {}", e),
        }
    }
    Ok(false)
}

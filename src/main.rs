use clap::{Parser, Subcommand};
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, Progress, RemoteCallbacks};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::{self, Write};
use std::{collections::HashMap, path::PathBuf};
use tempdir::TempDir;

struct State {
    progress: Option<Progress<'static>>,
    total: usize,
    current: usize,
    path: Option<PathBuf>,
    newline: bool,
}

fn print(state: &mut State) {
    let stats = state.progress.as_ref().unwrap();
    let network_pct = (100 * stats.received_objects()) / stats.total_objects();
    let index_pct = (100 * stats.indexed_objects()) / stats.total_objects();
    let co_pct = if state.total > 0 {
        (100 * state.current) / state.total
    } else {
        0
    };
    let kbytes = stats.received_bytes() / 1024;
    if stats.received_objects() == stats.total_objects() {
        if !state.newline {
            println!();
            state.newline = true;
        }
        print!(
            "Resolving deltas {}/{}\r",
            stats.indexed_deltas(),
            stats.total_deltas()
        );
    } else {
        print!(
            "net {:3}% ({:4} kb, {:5}/{:5})  /  idx {:3}% ({:5}/{:5})  \
             /  chk {:3}% ({:4}/{:4}) {}\r",
            network_pct,
            kbytes,
            stats.received_objects(),
            stats.total_objects(),
            index_pct,
            stats.indexed_objects(),
            stats.total_objects(),
            co_pct,
            state.current,
            state.total,
            state
                .path
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    }
    io::stdout().flush().unwrap();
}

fn clone(repo: &str) -> Result<TempDir, git2::Error> {
    let state = RefCell::new(State {
        progress: None,
        total: 0,
        current: 0,
        path: None,
        newline: false,
    });
    let mut cb = RemoteCallbacks::new();
    cb.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        print(&mut *state);
        true
    });

    let mut co = CheckoutBuilder::new();
    co.progress(|path, cur, total| {
        let mut state = state.borrow_mut();
        state.path = path.map(|p| p.to_path_buf());
        state.current = cur;
        state.total = total;
        print(&mut *state);
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    let clone_dir = TempDir::new(&slug::slugify(repo)).unwrap();
    RepoBuilder::new()
        .fetch_options(fo)
        .with_checkout(co)
        .clone(repo, clone_dir.path())?;
    println!();

    Ok(clone_dir)
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    PrepareMigration { umbrel_root: String },
    Migrate { umbrel_root: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UmbrelUserJson {
    name: String,
    password: String,
    unused_seed: bool,
    seed: String,
    repos: Vec<String>,
    remote_tor_access: bool,
    installed_apps: Vec<String>,
    app_origin: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct AppInfo {
    name: String,
    id: String,
    tagline: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct MigrationInfo {
    incompatible_apps: Vec<String>,
    experimental_apps: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    match args.command {
        Commands::PrepareMigration { umbrel_root } => {
            let mut unsupported_apps = Vec::new();
            let mut experimental_apps = Vec::new();

            let umbrel_root = PathBuf::from(umbrel_root);
            let user_json_path = umbrel_root.join("db").join("user.json");
            let user_json = std::fs::File::open(user_json_path).unwrap();
            let user_json: UmbrelUserJson = serde_json::from_reader(user_json).unwrap();
            let repos_dir = umbrel_root.join("repos");
            let apps_on_citadel = reqwest::blocking::get("https://runcitadel.space/api/apps")
                .unwrap()
                .json::<Vec<AppInfo>>()
                .unwrap()
                .iter()
                .map(|app| app.id.clone())
                .collect::<Vec<String>>();
            for app in user_json.installed_apps {
                if app == "bitcoin"
                    || app == "lightning"
                    || app == "electrum"
                    || apps_on_citadel.contains(&app)
                {
                    continue;
                }
                println!("App {} is not available on Citadel by default...", app);
                let origin = user_json.app_origin.get(&app).unwrap();
                let app_dir = repos_dir
                    .join(origin.replace(':', "-").replace('.', "-").replace('/', "-"))
                    .join(&app);
                let compose_yml = app_dir.join("docker-compose.yml");
                let compose_yml = std::fs::File::open(compose_yml).unwrap();
                let compose_yml: serde_yaml::Value = serde_yaml::from_reader(compose_yml).unwrap();
                for (_, service) in compose_yml.get("services").unwrap().as_mapping().unwrap() {
                    let mut keys = service.as_mapping().unwrap().keys();
                    let allowed_keys = vec![
                        "image",
                        "user",
                        "stop_grace_period",
                        "stop_signal",
                        "depends_on",
                        "network_mode",
                        "restart",
                        "init",
                        "extra_hosts",
                        "entrypoint",
                        "working_dir",
                        "command",
                        "environment",
                        "cap_add",
                        "volumes",
                        "networks",
                    ];
                    if keys.any(|k| !allowed_keys.contains(&k.as_str().unwrap())) {
                        unsupported_apps.push(app.clone());
                    }
                }
                if !unsupported_apps.contains(&app) {
                    experimental_apps.push(app);
                }
            }

            let migration_info = MigrationInfo {
                experimental_apps,
                incompatible_apps: unsupported_apps,
            };

            let info_file = umbrel_root.join("citadel.yml");
            let info_file = std::fs::File::create(info_file).expect("Failed to save migration info!");
            serde_yaml::to_writer(info_file, &migration_info).expect("Failed to save migration info!");
        }
        Commands::Migrate { umbrel_root } => {
            let citadel = clone("https://github.com/citadel-core/core").unwrap();
            println!("Cloned Citadel to {}", citadel.path().display());
            let umbrel_root = PathBuf::from(umbrel_root);
            std::fs::create_dir_all(umbrel_root.join("bitcoin")).unwrap();
            std::fs::create_dir_all(umbrel_root.join("lnd")).unwrap();
            let old_bitcoin_dir = umbrel_root
                .join("app-data")
                .join("bitcoin")
                .join("data")
                .join("bitcoin");
            let old_lightning_dir = umbrel_root
                .join("app-data")
                .join("lightning")
                .join("data")
                .join("lnd");
            // Move all files from old bitcoin dir to new bitcoin dir
            for entry in std::fs::read_dir(old_bitcoin_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let new_path = umbrel_root.join("bitcoin").join(path.file_name().unwrap());
                std::fs::rename(path, new_path).unwrap();
            }
            // Now, move all files from old lightning dir to new lightning dir
            for entry in std::fs::read_dir(old_lightning_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let new_path = umbrel_root.join("lnd").join(path.file_name().unwrap());
                std::fs::rename(path, new_path).unwrap();
            }
            // Rename app-data/electrum/data/electrs to app-data/electrs/data
            std::fs::create_dir_all(umbrel_root.join("app-data").join("electrs")).unwrap();
            let old_electrs_dir = umbrel_root
                .join("app-data")
                .join("electrum")
                .join("data")
                .join("electrs");
            let new_electrs_dir = umbrel_root.join("app-data").join("electrs").join("data");
            if old_electrs_dir.exists() {
                std::fs::rename(old_electrs_dir, new_electrs_dir).unwrap();
            }
            // Delete everything in umbrel_root except app-data, db, bitcoin and lnd
            for entry in std::fs::read_dir(&umbrel_root).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path == umbrel_root.join("app-data")
                    || path == umbrel_root.join("db")
                    || path == umbrel_root.join("bitcoin")
                    || path == umbrel_root.join("lnd")
                {
                    continue;
                }
                if path.is_dir() {
                    std::fs::remove_dir_all(path).unwrap();
                } else {
                    std::fs::remove_file(path).unwrap();
                }
            }
            // Copy all files from citadel to umbrel_root
            let files = std::fs::read_dir(citadel.path()).unwrap();
            fs_extra::copy_items(
                &files
                    .into_iter()
                    .map(|thing| thing.unwrap().path())
                    .collect::<Vec<PathBuf>>(),
                &umbrel_root,
                &fs_extra::dir::CopyOptions {
                    skip_exist: true,
                    ..Default::default()
                },
            )
            .unwrap();
            std::fs::remove_dir_all(umbrel_root.join("db").join("citadel-seed")).unwrap();
            std::fs::remove_dir_all(umbrel_root.join("sessions")).unwrap();
            std::fs::rename(
                umbrel_root.join("db").join("umbrel-seed"),
                umbrel_root.join("db").join("citadel-seed"),
            )
            .unwrap();
            println!("Migrated sucessfully!");
            reqwest::Client::new().post("https://myhlcivijzochaekxdhp.functions.supabase.co/add-umbrel-user").bearer_auth("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im15aGxjaXZpanpvY2hhZWt4ZGhwIiwicm9sZSI6ImFub24iLCJpYXQiOjE2NDk5MzQzMzcsImV4cCI6MTk2NTUxMDMzN30.aK3O5JCcQ2qacykGWMGhtdupLd3KsB6P-rHVxAmwPsw").send().await.unwrap();
        }
    }
}

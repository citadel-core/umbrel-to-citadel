use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CheckMigration { umbrel_root: String },
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

fn main() {
    let args = Cli::parse();
    match args.command {
        Commands::CheckMigration { umbrel_root } => {
            let mut unsupported_apps = Vec::new();

            let umbrel_root = PathBuf::from(umbrel_root);
            let user_json_path = umbrel_root.join("db/user.json");
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
            }

            println!("Unsupported apps: {:?}", unsupported_apps);
        }
    }
}

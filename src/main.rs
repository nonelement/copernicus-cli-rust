extern crate chrono;
extern crate clap;
extern crate colored;
extern crate confy;
extern crate dotenv;
extern crate env_logger;
extern crate futures_util;
extern crate geojson;
extern crate log;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate spinners;
extern crate tokio;
extern crate url;

mod args;
mod api;
mod util;

use std::env::var;
use std::error::Error;

use clap::Parser;
use dotenv::dotenv;
use log::info;
use serde::{Serialize, Deserialize};
use spinners::{Spinner, Spinners};

use args::{CliArgs, Mode};
use api::{AuthDetails, check_auth, download_imagery, search_imagery};
use util::format_feature_collection;

const APP_NAME: &str = "COPERNICUS-CLI";
const ENV_VAR_USER: &str = "COPERNICUS_USER";
const ENV_VAR_PASS: &str = "COPERNICUS_PASS";


#[derive(Serialize, Deserialize, Debug)]
struct Config {
    version: u8,
    auth_details: Option<AuthDetails>,
}

impl ::std::default::Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            auth_details: Option::None
        }
    }
}

fn get_env_creds() -> Credentials {
    Credentials {
        user: var(ENV_VAR_USER).ok(),
        pass: var(ENV_VAR_PASS).ok()
    }
}

// Related to both CLI ENV and Auth interactions
struct Credentials {
    pub user: Option<String>,
    pub pass: Option<String>
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    dotenv().ok();

    // Use Result type instead, use ? here to exit immediately
    // let args = get_args()?;
    let args = CliArgs::parse();

    let mut config: Config = confy::load(APP_NAME, None)?;
    let credentials = get_env_creds();

    // Check provided user name to see if it has a reasonable value, e.g. not
    // the template value, and not None. If it doesn't, we can't auth. We could
    // reauth within the refresh window with cached auth, but we can probably
    // insist on this.
    match credentials.user {
        Some(ref user) => if user == "FAKE_USER" {
            panic!("Template value present in env credentials. Check values?");
        },
        None => panic!("No env value for user. Check credentials.")
    }

    let client = reqwest::Client::new();

    info!("Checking auth...");
    let auth_details = check_auth(config.auth_details, &credentials).await?;
    info!("Auth ok!");

    // Save auth details
    config.auth_details = Some(auth_details.clone());
    confy::store(APP_NAME, None, config)?;

    match args.mode {
        Mode::Search(search_args) => {
            let mut s = Spinner::new(Spinners::Dots, "Searching for imagery...".into());
            let fc = search_imagery(&client, &auth_details, search_args.into()).await?;
            s.stop_with_newline();
            println!("Search results:\n{}", format_feature_collection(&fc));
            Ok(())
        },
        Mode::Download(download_args) => {
            let mut s = Spinner::new(Spinners::Dots, "Querying for imagery with id...".into());
            let fc = search_imagery(&client, &auth_details, download_args.clone().into()).await?;
            s.stop_with_newline();
            if fc.features.is_empty() {
                return Err(format!("No imagery found for id: {:?}", download_args.ids).into());
            }
            let details = download_imagery(&client, &auth_details, &fc.features[0], download_args.output_dir).await?;
            println!("{} bytes, saved to: {}", details.size, details.destination.to_str().unwrap_or("_"));
            Ok(())
        },
    }
}


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

use dotenv::dotenv;
use log::info;
use serde::{Serialize, Deserialize};
use spinners::{Spinner, Spinners};

use args::{ModeIntent, get_args};
use api::{AuthDetails, check_auth, download_imagery, list_imagery, search_imagery};
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
    let args = get_args();

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

    match args.intent {
        ModeIntent::List => {
            let mut s = Spinner::new(Spinners::Dots, "Querying for imagery...".into());
            let fc = list_imagery(&client, &auth_details, args.into()).await?;
            s.stop_with_newline();
            println!("List results:\n{}", format_feature_collection(&fc));
            Ok(())
        },
        ModeIntent::Search => {
            let mut s = Spinner::new(Spinners::Dots, "Searching for imagery...".into());
            let fc = search_imagery(&client, &auth_details, args.into()).await?;
            s.stop_with_newline();
            println!("Search results:\n{}", format_feature_collection(&fc));
            Ok(())
        },
        ModeIntent::Download => {
            let mut s = Spinner::new(Spinners::Dots, "Querying for imagery with id...".into());
            let fc = search_imagery(&client, &auth_details, args.clone().into()).await?;
            s.stop_with_newline();
            if fc.features.is_empty() {
                return Err(format!("No imagery found for id: {:?}", args.ids).into());
            }
            let mut s = Spinner::new(Spinners::Dots, "Downloading imagery...".into());
            let details = download_imagery(&client, &auth_details, &fc.features[0], args.output_dir).await?;
            s.stop_with_newline();
            println!("Download complete.");
            println!("{} bytes, saved to: {}", details.size, details.destination.to_str().unwrap_or("_"));
            Ok(())
        },
        ModeIntent::Error(reason) => Err(format!("Something went wrong: {reason}").into()),
        _ => Ok(()) // Catch for all other types, like unknown or NYIs.
    }
}


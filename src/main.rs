#[macro_use]
extern crate clap;
extern crate colored;
extern crate confy;
extern crate dotenv;
extern crate env_logger;
extern crate geojson;
extern crate log;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate spinners;
extern crate tokio;

use std::env::var;
use std::error::Error;
use dotenv::dotenv;
use serde::{Serialize, Deserialize};
use spinners::{Spinner, Spinners};
use clap::{ArgGroup, Command, Parser};

mod api;
mod util;

use api::{AuthDetails, Credentials, check_auth, list_imagery};
use util::format_feature_collection;

const APP_NAME: &str = "COPERNICUS-CLI";
const COMMAND_NAME: &str = "copernicus";
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

// A cli app to search for copernicus data
#[derive(Parser, Debug)]
struct Args {
    // A bounding box to search by
    #[arg(long)]
    bbox: Option<String>,
    #[arg(long)]
    datetime: Option<String>,
}

fn get_env_creds() -> Credentials {
    Credentials {
        user: var(ENV_VAR_USER).ok(),
        pass: var(ENV_VAR_PASS).ok()
    }
}

fn get_args() -> Args {
    let parsed = Command::new(COMMAND_NAME)
        .arg(arg!(--bbox <bbox> "provide a bounding box"))
        .arg(arg!(--datetime <datetime> "filter by datetime"))
        .group(ArgGroup::new("required.args")
            .args(["bbox", "datetime"])
            .required(true)
            .multiple(true)
        ).get_matches();

    let args = Args {
        bbox: parsed.get_one::<String>("bbox").cloned(),
        datetime: parsed.get_one::<String>("datetime").cloned()
    };


    return args;
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

    let mut s = Spinner::new(Spinners::Dots, "Checking auth...".into());
    let auth_details = check_auth(config.auth_details, &credentials).await?;
    s.stop_with_newline();

    // Save auth details
    config.auth_details = Some(auth_details.clone());
    confy::store(APP_NAME, None, config)?;

    let mut s = Spinner::new(Spinners::Dots, "Querying for imagery at bbox...".into());
    let fc = list_imagery(&client, &auth_details, args.bbox.unwrap()).await?;
    s.stop_with_newline();
    println!("features:\n{}", format_feature_collection(&fc));

    Ok(())
}


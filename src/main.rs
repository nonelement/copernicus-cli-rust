extern crate chrono;
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
extern crate url;

mod api;
mod util;

use std::env::var;
use std::error::Error;

use chrono::offset::Utc;
use chrono::DateTime;
use dotenv::dotenv;
use serde::{Serialize, Deserialize};
use spinners::{Spinner, Spinners};
use clap::{Arg, ArgGroup, Command};

use api::{AuthDetails, Credentials, check_auth, list_imagery};
use util::{parse_date, format_feature_collection};

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
#[derive(Debug)]
struct Args {
    id: Option<String>,
    bbox: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    sortby: Option<String>,
    page: Option<u16>,
    limit: Option<u16>
}

fn get_env_creds() -> Credentials {
    Credentials {
        user: var(ENV_VAR_USER).ok(),
        pass: var(ENV_VAR_PASS).ok()
    }
}

fn parse_datetime(arg: Option<String>) -> Result<DateTime<Utc>, Box<dyn Error>> {
    if let Some(datetime_string) = arg {
        parse_date(datetime_string)
    } else {
        Err("Unable to parse datetime arg.".into())
    }

}

fn parse_u16(arg: Option<String>) -> Result<u16, Box<dyn Error>> {
    if let Some(u16_string) = arg {
        match u16_string.parse::<u16>() {
            Ok(v) => Ok(v),
            Err(_) => Err("Unable to parse u16 value.".into())
        }
    } else {
        Err("Unable to parse u16 value.".into())
    }
}

fn get_args() -> Args {
    // Requires one flag of: bbox, datetime, to, from
    // datetime is exclusive with to and from
    let matched = Command::new(COMMAND_NAME)
        .arg(Arg::new("id")
                .long("id")
                .help("id to search for")
        )
        .arg(Arg::new("bbox")
                .long("bbox")
                .help("provides a bounding box for the query(top left, bottom right)")
        )
        .arg(Arg::new("from")
                .long("from")
                .help("start of range to query by: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DD")
        )
        .arg(Arg::new("to")
                .long("to")
                .help("end of range to query by: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DD")
        )
        .arg(Arg::new("sortby")
                .long("sortby")
                .help("sort query results by direction, field. [+|-][start_datetime | end_datetime | datetime]")
        )
        .arg(Arg::new("limit")
                .long("limit")
                .help("limit on the number of items returned")
        )
        .arg(Arg::new("page")
                .long("page")
                .help("provides the page number to retrieve for paginated responses")
        )
        .group(ArgGroup::new("required.args")
            .args(["id", "bbox", "from", "to"])
            .required(true)
            .multiple(true)
        )
        .get_matches();

    let id = matched.get_one::<String>("id").cloned();
    let bbox = matched.get_one::<String>("bbox").cloned();
    let from = parse_datetime(matched.get_one::<String>("from").cloned()).ok();
    let to = parse_datetime(matched.get_one::<String>("to").cloned()).ok();
    let sortby = matched.get_one::<String>("sortby").cloned();
    let limit = parse_u16(matched.get_one::<String>("limit").cloned()).ok();
    let page = parse_u16(matched.get_one::<String>("page").cloned()).ok();

    return Args { id, bbox, from, to, sortby, limit, page };
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
    let fc = list_imagery(&client, &auth_details, args.into()).await?;
    s.stop_with_newline();
    println!("features:\n{}", format_feature_collection(&fc));

    Ok(())
}


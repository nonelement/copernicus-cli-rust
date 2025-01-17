use std::error::Error;

use chrono::offset::Utc;
use chrono::DateTime;
use clap::{Arg, ArgMatches, Command};
use log::{error, debug};

use crate::util::parse_date;


const COMMAND_NAME: &str = "copernicus";

/*
 * Enum representing an interpreted user intent. Used to signal that we should
 * behave as though asked to list, or search, or download, depending on args or
 * provided subcommands.
 */
#[derive(Debug, Clone)]
pub enum ModeIntent {
    List,
    // TODO: Search,
    Download,
    Error(String),
    Unknown,
}


#[derive(Debug, Clone)]
pub struct Args {
    pub intent: ModeIntent,
    pub ids: Option<String>,
    pub collection: Option<String>,
    pub bbox: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sortby: Option<String>,
    pub page: Option<u16>,
    pub limit: Option<u16>
}

/*
 * Parses a string as a datetime.
 * We parse this value to generate floor or ceil values, if just dates are given.
 */
fn parse_datetime(arg: Option<String>) -> Result<DateTime<Utc>, Box<dyn Error>> {
    if let Some(datetime_string) = arg {
        parse_date(datetime_string)
    } else {
        Err("Unable to parse datetime arg.".into())
    }

}

/*
 * Parses an arg as a number.
 * Note: We're mostly just passing this through to the request at this point,
 * but parsing this as a number affords us some control over what we do with it.
 * For example, if we wanted to add a tool-specific query limit of 10 or 20, its
 * easier to do this if it's parsed and typed correctly.
 */
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

// Retrieves args from a match / submatch
fn get_standard_args(m: &ArgMatches) -> Args {
    let intent = ModeIntent::Unknown;
    let collection = None;
    // Options
    let ids = m.get_one::<String>("ids").cloned();
    let bbox = m.get_one::<String>("bbox").cloned();
    let from = parse_datetime(m.get_one::<String>("from").cloned()).ok();
    let to = parse_datetime(m.get_one::<String>("to").cloned()).ok();
    let sortby = m.get_one::<String>("sortby").cloned();
    let limit = parse_u16(m.get_one::<String>("limit").cloned()).ok();
    let page = parse_u16(m.get_one::<String>("page").cloned()).ok();

    Args { intent, ids, collection, bbox, from, to, sortby, limit, page }
}

// Extracts arguments from clap::ArgMatches for each subcommand.
fn get_args_from_match(am: ArgMatches) -> Result<Args, Box<dyn Error>> {
    let collection_default = String::from("SENTINEL-2");
    match am.subcommand() {
        Some(("list", submatch)) => {
            // Options
            let mut args = get_standard_args(submatch);
            // Settings w defaults
            args.intent = ModeIntent::List;
            args.collection = submatch.get_one::<String>("collection").cloned()
                .or(Some(collection_default));
            Ok(args)
        },
        Some(("download", submatch)) => {
            // Options
            let mut args = get_standard_args(submatch);
            // Settings w defaults
            args.intent = ModeIntent::Download;
            args.collection = None;
            Ok(args)
        },
        Some((invalid, _submatch)) => {
            Err(format!("Not a valid subcommand: {invalid}").into())
        },
        None => {
            // Options
            let mut args = get_standard_args(&am);
            // Settings w defaults
            args.intent = ModeIntent::List;
            args.collection = am.get_one::<String>("collection").cloned()
                .or(Some(collection_default));
            Ok(args)
        }
    }
}

// Applies common filter arguments to a command, since these'll .
fn apply_filter_args(c: Command) -> Command {
    c.arg(Arg::new("ids")
            .long("ids")
            .help("id to search for")
    )
    .arg(Arg::new("bbox")
            .long("bbox")
            .allow_negative_numbers(true)
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
}

pub fn get_args() -> Args {
    // TODO: Add subcommand for the search endpoint here.
    let matches = apply_filter_args(Command::new(COMMAND_NAME))
        .subcommand(
            apply_filter_args(Command::new("list"))
                .arg(Arg::new("collection")
                    .long("collection")
                    .help("specify which collection to query. Default: SENTINEL-2")
                )
                .about("List imagery from a specific collection")
        )
        .subcommand(
            Command::new("download")
                .arg(Arg::new("ids")
                    .long("ids")
                    .help("specify which products to download")
                )
                .about("Download imagery using ids obtained through <list>")
        )
        // TODO duplicate for compatibility with subcommandless invocation
        .arg(Arg::new("collection")
            .long("collection")
            .help("specify which collection to query. Default: SENTINEL-2")
        )
        .get_matches();

    debug!("parsed args:\n{:#?}", matches);

    match get_args_from_match(matches) {
        Ok(a) => a,
        Err(e) => {
            error!("Unable to parse arguments: {e}");
            Args {
                intent: ModeIntent::Error(String::from("Unable to parse arguments")),
                ids: None,
                collection: None,
                bbox: None,
                from: None,
                to: None,
                sortby: None,
                limit: None,
                page: None,
            }
        }
    }
}

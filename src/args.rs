use std::error::Error;
use std::default::Default;

use chrono::offset::Utc;
use chrono::DateTime;
use clap::{Args, Parser, Subcommand};

use crate::util::parse_date;


#[derive(Clone, Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Mode {
    Search(SearchArgs),
    Download(DownloadArgs),
}


#[derive(Clone, Debug, Default, Args)]
pub struct SearchArgs {
    #[arg(long = "ids")]
    pub ids: Option<String>,
    #[arg(long, help = "the collection to search, e.g. SENTINEL-2")]
    pub collections: Option<String>,
    #[arg(long, help = "provides a bounding box for the query(top left, bottom right)")]
    pub bbox: Option<String>,
    #[arg(
        long,
        help = "start of range to query by: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DD",
        value_parser = |s: &str| parse_datetime(s, Some(TimeAdjust::Floor))
    )]
    pub from: Option<DateTime<Utc>>,
    #[arg(
        long,
        help = "end of range to query by: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DD",
        value_parser = |s: &str| parse_datetime(s, Some(TimeAdjust::Ceil))
    )]
    pub to: Option<DateTime<Utc>>,
    #[arg(long, help = "sort query results by direction, field. [+|-][start_datetime | end_datetime | datetime]")]
    pub sortby: Option<String>,
    #[arg(long, help = "which page to fetch for paginated responses")]
    pub page: Option<u16>,
    #[arg(long, help = "limit on the number of items returned")]
    pub limit: Option<u16>,
}

#[derive(Clone, Debug, Default, Args)]
pub struct DownloadArgs {
    #[arg(long = "ids")]
    pub ids: Option<String>,
    #[arg(short = 'o', long = "output", help = "Where to write files")]
    pub output_dir: Option<String>,
}

/*
 * impl to make converting from passed args to search params easy.
 */
impl From<DownloadArgs> for SearchArgs {
    fn from(da: DownloadArgs) -> Self {
        SearchArgs { ids: da.ids, ..Default::default() }
    }
}


pub enum TimeAdjust {
    Floor,
    Ceil
}

/*
 * Parses a string as a datetime.
 * We parse this value to generate floor or ceil values, if just dates are given.
 */
fn parse_datetime(datetime_str: &str, should_adjust: Option<TimeAdjust>) -> Result<DateTime<Utc>, Box<dyn Error + Send + Sync>> {
    match parse_date(datetime_str, should_adjust) {
        Ok(dt) => Ok(dt),
        Err(e) => Err(e)
    }
}


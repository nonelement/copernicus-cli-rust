use std::collections::HashMap;
use std::error::Error;

use chrono::offset::Utc;
use chrono::{DateTime, NaiveDate};
use colored::Colorize;
use geojson::{Feature, FeatureCollection};
use geojson::feature::Id;
use geojson::JsonObject;
use geojson::JsonValue;
use serde_json::Map;
use serde_json::Value;

use crate::args::TimeAdjust;
/*
 * Hardcoded style information for List and Search outputs. At the moment these
 * are all set to conservative (read: useless?) values.
 * TODO: Refine this.
 */
const STYLES: [(&str, &str); 9] = [
    ("ID", "White"),
    ("SHORT_NAME", "White"),
    ("SERIAL", "White"),
    ("DETAIL", "White"),
    ("CAPTURE_TIME", "White"),
    ("CLOUD_COVER", "White"),
    ("BBOX", "White"),
    ("QUICKLOOK_HREF", "White"),
    ("PRODUCT_HREF", "White"),
];

/*
 * Singular template to use for listing features
 * TODO: Use different templates for different types of features
 */
const FEATURE_DETAILS_FORMAT: &str = r#"
<ID> (<SHORT_NAME>.<SERIAL>/<DETAIL>)
  <CAPTURE_TIME> cloudy: <CLOUD_COVER>
  bbox: <BBOX>
  quicklook: <QUICKLOOK_HREF>
  product: <PRODUCT_HREF>
"#;

/*
 * Function to map color values to Colorize function calls, which colors output
 * strings. This might not include all the available colors from Colorize, just
 * those I tested with.
 */
fn style_value(k: &str, v: String, styles: &HashMap<&str, &str>) -> String {
    let style = styles.get(k);
    match style {
        Some(s) => {
            match *s {
                "White" => v.as_str().white().to_string(),
                "BrightWhite" => v.as_str().bright_white().to_string(),
                "BrightBlack" => v.as_str().bright_black().to_string(),
                "Green" => v.as_str().green().to_string(),
                "Cyan" => v.as_str().cyan().to_string(),
                "BrightCyan" => v.as_str().bright_cyan().to_string(),
                "Blue" => v.as_str().blue().to_string(),
                "Purple" => v.as_str().purple().to_string(),
                "BrightBlue" => v.as_str().bright_blue().to_string(),
                "Red" => v.as_str().red().to_string(),
                _ => v
            }
        },
        None => v,
    }
}

/*
 * Converts feature ids to a string here for display. GeoJSON is flexible about
 * typing, but we just want strings here, since we're just gonna print them out.
 */
pub fn get_id(id: &Option<Id>) -> Option<String> {
    match id {
        Some(Id::String(v)) => Some(v.clone()),
        Some(Id::Number(n)) => Some(n.to_string()),
        None => None
    }
}

// Path into a geojson::JsonObject to retrieve a nested value
pub fn from_path(path: Vec<&str>, m: &Option<JsonObject>) -> Option<Value> {
    let mut v: &JsonObject = if let Some(v) = m { v } else { return None };
    let mut t: Option<JsonValue> = Some(Value::Null);
    for name in path {
        let _v: &Value = if let Some(name) = &v.get(name) { name } else { return None };
        match _v {
            serde_json::Value::Object(obj) => v = obj,
            other => t = Some(other.clone()),
        }
    }
    t
}

// Unwraps serde_json::Value and converts it to a string for display
pub fn get_value(value_opt: Option<Value>) -> Option<String> {
    if let Some(value) = value_opt {
        match value {
            Value::String(v) => Some(v.to_string()),
            Value::Number(v) => Some(v.to_string()),
            Value::Bool(v) => Some(v.to_string()),
            // Vec<Value> type. Lightly recurse to get leaf values.
            Value::Array(v) => Some(
                v.iter().map(|subv|
                    get_value(Some(subv.clone())
                ).unwrap()) // Unwrapping, but this always returns Option<String>
                .collect::<Vec<String>>()
                .join(", ")
            ),
            // Possibly object type or Null. Short circuit with "N/A" for now.
            _ => Some(String::from("N/A")),
        }
    } else {
        Some(String::from("N/A"))
    }
}

/*
 * Parses a specific datetime format OR a date value into a datetime value.
 * There's some added convenience here for converting dates into datetimes by
 * getting min or max time values, which are usually a bit annoying to type out
 * over and over if working from the CLI.
 */
pub fn parse_date(s: &str, should_adjust: Option<TimeAdjust>) -> Result<DateTime<Utc>, Box<dyn Error + Send + Sync>> {
    let parsed = DateTime::parse_from_rfc3339(s); // Subset of ISO 8601
    match parsed {
        Ok(dt) => Ok(dt.into()),
        Err(_e) => {
            // Parse a date, then zero out the time and convert to DateTime<Utc>
            let parsed = NaiveDate::parse_from_str(s, "%F");
            if let Ok(dt) = parsed {
                match should_adjust {
                    Some(TimeAdjust::Floor) => Ok(dt.and_hms_opt(0,0,0).unwrap().and_utc()),
                    Some(TimeAdjust::Ceil) => Ok(dt.and_hms_opt(23,59,59).unwrap().and_utc()),
                    // If no adjustment was requested but we have a short date, we still have to
                    // apply a value here, and this might be the most sensible for ranges.
                    None => Ok(dt.and_hms_opt(0,0,0).unwrap().and_utc()),
                }
            } else {
                Err(format!("Unable to parse: {s}").into())
            }
        }
    }
}

// Display methods

/*
 * List and Serach endpoints will return feature collections, so this is a top
 * level display function so that we can just print out whatever came back
 * for the provided query.
 */
pub fn format_feature_collection(fc: &FeatureCollection) -> String {
    let mut output: Vec<String> = Vec::new();
    for feature in fc.features.clone() {
        output.push(format_feature(&feature));
    }
    output.join("\n")
}

/*
 * Feature display method. Extracts information from the feature and passes
 * it along to the templating function to generate finalized output.
 */
pub fn format_feature(f: &Feature) -> String {
    // Top level feature attributes
    let id = get_id(&f.id);
    let bbox = Some(f.bbox.clone().unwrap_or_default().iter().map(|&v| v.to_string()).collect::<Vec<String>>().join(","));
    // Feature properties:
    let properties = if let Some(properties) = &f.properties { properties } else { &Map::new() };
    let short_name: Option<String> = get_value(properties.get("platformShortName").cloned());
    let serial_identifier: Option<String> = get_value(properties.get("platformSerialIdentifier").cloned());
    let product_type: Option<String> = get_value(properties.get("productType").cloned());
    let capture_time: Option<String> = get_value(properties.get("datetime").cloned());
    // Atmospheric values
    let cloud_cover: Option<String> = get_value(properties.get("cloudCover").cloned());
    // Product links
    let quicklook_href: Option<String> = get_value(from_path(Vec::from(["assets", "QUICKLOOK", "href"]), &f.foreign_members));
    let product_href: Option<String> = get_value(from_path(Vec::from(["assets", "PRODUCT", "href"]), &f.foreign_members));
    let data = HashMap::from([
        ("ID", id),
        ("SHORT_NAME", short_name),
        ("SERIAL", serial_identifier),
        ("DETAIL", product_type),
        ("CAPTURE_TIME", capture_time),
        ("CLOUD_COVER", cloud_cover),
        ("BBOX", bbox),
        ("QUICKLOOK_HREF", quicklook_href),
        ("PRODUCT_HREF", product_href)
    ]);
    format_with_template(FEATURE_DETAILS_FORMAT, &data)
}

/*
 * Takes a template and a HashMap of values and interpolates them. Will also use
 * the STYLES information at the top of the file to colorize output, though how
 * useful this is depends on the end users' terminal configuration.
 */
fn format_with_template(template: &str, data: &HashMap<&str, Option<String>>) -> String {
    let mut compiled = String::from(template).truecolor(64, 64, 64).to_string();
    let styles = HashMap::from(STYLES);
    for (k, mv) in data {
        let v = if let Some(v) = mv { v } else { &String::from("N/A") };
        let tag = format!("<{k}>");
        let value = style_value(k, v.clone(), &styles);
        compiled = compiled.replace(&tag, &value);
    }
    compiled.to_string()
}


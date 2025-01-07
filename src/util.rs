use std::collections::HashMap;
use std::error::Error;

use chrono::offset::Utc;
use chrono::{DateTime, NaiveDate};
use colored::Colorize;
use geojson::{Feature, FeatureCollection};
use geojson::feature::Id;
use geojson::JsonObject;
use geojson::JsonValue;
use serde_json::map::Map;
use serde_json::Value;

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


const FEATURE_DETAILS_FORMAT: &str = r#"
<ID> (<SHORT_NAME>.<SERIAL>/<DETAIL>)
  <CAPTURE_TIME> cloudy: <CLOUD_COVER>
  bbox: <BBOX>
  quicklook: <QUICKLOOK_HREF>
  archive: <PRODUCT_HREF>
"#;

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

pub fn format_feature_collection(fc: &FeatureCollection) -> String {
    let mut output: Vec<String> = Vec::new();
    for feature in fc.features.clone() {
        output.push(format_feature(&feature));
    }
    output.join("\n")
}

fn from_path(path: Vec<&str>, m: &Option<JsonObject>) -> Option<Value> {
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

fn get_id(id: &Option<Id>) -> Option<String> {
    match id {
        Some(Id::String(v)) => Some(v.clone()),
        Some(Id::Number(n)) => Some(n.to_string()),
        None => None
    }
}

fn get_value(value_opt: Option<Value>) -> Option<String> {
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

pub fn format_feature(f: &Feature) -> String {
    // Top level feature attributes
    let id = get_id(&f.id);
    let bbox = Some(f.bbox.clone().unwrap_or_default().iter().map(|&v| v.to_string()).collect::<Vec<String>>().join(","));
    // Feature properties:
    let properties = if let Some(properties) = &f.properties { properties } else { &Map::new() };
    let short_name: Option<String> = get_value(properties.get("platformShortName").cloned());
    let serial_identifier: Option<String> = get_value(properties.get("platformSerialIdentifier").cloned());
    let product_type: Option<String> = get_value(properties.get("productType").cloned());
    // Sentinel-2 Value
    let cloud_cover: Option<String> = get_value(properties.get("cloudCover").cloned());
    let capture_time: Option<String> = get_value(properties.get("datetime").cloned());
    let quicklook_href: Option<String> = get_value(from_path(vec!["assets", "QUICKLOOK", "href"], &f.foreign_members));
    let product_href: Option<String> = get_value(from_path(vec!["assets", "PRODUCT", "href"], &f.foreign_members));
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

// Try this with pure accessor methods and the template at the top
fn format_with_template(template: &str, data: &HashMap<&str, Option<String>>) -> String {
    let mut compiled = String::from(template).truecolor(64, 64, 64).to_string();
    let styles = HashMap::from(STYLES);
    for (k, mv) in data {
        let v = if let Some(v) = mv { v } else { &String::from("N/A") };
        let tag = format!("<{}>", k);
        let value = style_value(k, v.clone(), &styles);
        compiled = compiled.replace(&tag, &value);
    }
    compiled.to_string()
}

pub fn parse_date(s: String) -> Result<DateTime<Utc>, Box<dyn Error>> {
    let s = s.as_str();
    let parsed = DateTime::parse_from_rfc3339(s); // Subset of ISO 8601
    match parsed {
        Ok(dt) => Ok(dt.into()),
        Err(_e) => {
            // Parse a date, then zero out the time and convert to DateTime<Utc>
            let parsed = NaiveDate::parse_from_str(s, "%F");
            if let Ok(dt) = parsed {
                Ok(dt.and_hms_opt(0,0,0).unwrap().and_utc())
            } else {
                Err(format!("Unable to parse: {}", s).into())
            }
        }
    }
}




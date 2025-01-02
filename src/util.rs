use std::collections::HashMap;
use std::error::Error;

use chrono::offset::Utc;
use chrono::DateTime;
use colored::Colorize;
use geojson::{Feature, FeatureCollection};
use geojson::feature::Id;
use geojson::JsonObject;
use geojson::JsonValue;
use serde_json::map::Map;
use serde_json::Value;

const DTZ_FORMAT: &'static str = "%Y-%M-%DT%h:%m:%sZ";
const DZ_FORMAT: &'static str = "%Y-%M-%D";

const STYLES: [(&'static str, &'static str); 9] = [
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
    return output.join("\n");
}

fn get_value(path: Vec<&str>, m: &Option<JsonObject>) -> JsonValue {
    let mut v: &JsonObject = if let Some(v) = m { v } else { return JsonValue::Null };
    let mut t: JsonValue = Value::Null;
    for name in path {
        let _v: &JsonValue = &v[name];
        match _v {
            serde_json::Value::Object(obj) => v = &obj,
            other => t = other.clone(),
        }
    }
    return t;
}

pub fn format_feature(f: &Feature) -> String {
    // Top level feature attributes
    let id = match &f.id { Some(Id::String(v)) => v.clone(), Some(Id::Number(n)) => n.to_string(), None => String::new() };
    let bbox = f.bbox.clone().unwrap_or(vec![]).iter().map(|&v| v.to_string()).collect::<Vec<String>>().join(",");
    // Feature properties:
    let ref properties = if let Some(properties) = &f.properties { properties } else { &Map::new() };
    let short_name: String = String::from(properties["platformShortName"].as_str().unwrap());
    let serial_identifier: String = String::from(properties["platformSerialIdentifier"].as_str().unwrap());
    let product_type: String = String::from(properties["productType"].as_str().unwrap());
    let cloud_cover: String = properties["cloudCover"].as_f64().unwrap().to_string();
    let capture_time: String = String::from(properties["datetime"].as_str().unwrap());
    let quicklook_href: String = String::from(get_value(vec!["assets", "QUICKLOOK", "href"], &f.foreign_members).as_str().unwrap());
    let product_href: String = String::from(get_value(vec!["assets", "PRODUCT", "href"], &f.foreign_members).as_str().unwrap());
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
    return format_with_template(FEATURE_DETAILS_FORMAT, &data);
}

// Try this with pure accessor methods and the template at the top
fn format_with_template(template: &str, data: &HashMap<&str, String>) -> String {
    let mut compiled = String::from(template).truecolor(64, 64, 64).to_string();
    let styles = HashMap::from(STYLES);
    for (k, v) in data {
        let tag = format!("<{}>", k);
        let value = style_value(&k, v.clone(), &styles);
        compiled = compiled.replace(&tag, &value);
    }
    return compiled.to_string();
}

// Parsing

pub fn get_date(s: String) -> Result<DateTime<Utc>, Box<dyn Error>> {
    let s = s.as_str();
    let parsed = DateTime::parse_from_str(s, DTZ_FORMAT);
    match parsed {
        Ok(dt) => Ok(dt.into()),
        Err(_e) => {
            let parsed = DateTime::parse_from_str(s, DZ_FORMAT);
            if let Ok(dt) = parsed {
                Ok(dt.into())
            } else {
                Err(format!("Unable to parse: {}", s).into())
            }
        }
    }
}

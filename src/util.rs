use geojson::{Feature, FeatureCollection};
use geojson::feature::Id;
use serde_json::map::Map;

pub fn format_feature_collection(fc: &FeatureCollection) -> String {
    let mut output: Vec<String> = Vec::new();
    for feature in fc.features.clone() {
        output.push(format_feature(&feature));
    }
    return output.join("\n");
}

pub fn format_feature(f: &Feature) -> String {
    // Top level feature attributes
    let id = match &f.id { Some(Id::String(v)) => v.clone(), Some(Id::Number(n)) => n.to_string(), None => String::new() };
    let bbox = f.bbox.clone().unwrap_or(vec![]).iter().map(|&v| v.to_string()).collect::<Vec<String>>().join(", ");
    // Feature properties:
    let ref properties = if let Some(properties) = &f.properties { properties } else { &Map::new() };
    let platform_name: &str = properties["platformShortName"].as_str().unwrap();
    let cloud_cover: f64 = properties["cloudCover"].as_f64().unwrap();
    let capture_time: &str = properties["datetime"].as_str().unwrap();
    return format!(
        "type: {}, id: {}, cloudy: {}\ncapture time: {}, bbox: {}\n",
        platform_name, id, cloud_cover, capture_time, bbox
    );
}

use std::collections::HashMap;
use std::convert::From;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::offset::Utc;
use chrono::{DateTime, SecondsFormat::Secs};
use futures_util::StreamExt;
use geojson::{Feature, FeatureCollection, GeoJson};
use log::{debug, info, error};
use reqwest::{Client, Response};
use serde::{Serialize, Deserialize};
use url::Url;

use crate::Credentials;
use crate::args::{DownloadArgs, SearchArgs};
use crate::util::{get_id, get_value, from_path};

// POST
const AUTH_URL: &str = "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";
// GET
// LIST_URL is a template and requires a Collection ID, e.g. SENTINEL-2
const SEARCH_URL: &str = "https://catalogue.dataspace.copernicus.eu/stac/search";

// Core auth struct. Gets saved and updated each run with new information.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthDetails {
    #[serde(default)]
    pub acquired_time: i64, // When authentication was acquired, to check current age
    pub access_token: String,
    pub expires_in: i32,
    pub refresh_token: String,
    pub refresh_expires_in: i64,
    pub token_type: String,
    #[serde(rename(serialize = "not-before-policy", deserialize = "not-before-policy"))]
    pub not_before_policy: i32,
    pub session_state: String,
    pub scope: String
}

// Internal to Api, Auth code, which helps us reason about auth state.
enum AuthState {
    IsOK,
    NeedsRefresh,
    NeedsReauthentication,
}

// Authentication

/*
 * We save some timestamps on our auth object so we can know whether we have to
 * refresh, reacquire, or can just use the saved auth details.
 */
fn get_auth_state(auth_details: &AuthDetails) -> Result<AuthState, Box<dyn Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let is_expired = now > (auth_details.acquired_time + auth_details.expires_in as i64).try_into()?;
    let is_refresh_expired = now > (auth_details.acquired_time + auth_details.refresh_expires_in).try_into()?;
    match (is_expired, is_refresh_expired) {
        (false, false) => Ok(AuthState::IsOK),
        (true, false) => Ok(AuthState::NeedsRefresh),
        (true, true) => Ok(AuthState::NeedsReauthentication),
        // Auth is in some other state and we should probably reauth
        _ => Ok(AuthState::NeedsReauthentication)
    }
}

/*
 * Checks the auth object and does whatever's necessary to get a working auth value.
 */
pub async fn check_auth(auth_details: Option<AuthDetails>, credentials: &Credentials) -> Result<AuthDetails, Box<dyn Error>> {
    match auth_details {
        None => {
            // Acquire auth
            authenticate_credentials(credentials).await
        },
        Some(auth_details) => {
            match get_auth_state(&auth_details) {
                Ok(auth_state) => {
                    match auth_state {
                        AuthState::IsOK => {
                            debug!("Auth: Existing auth ok, resuing.");
                            Ok(auth_details) // TODO: This returns the moved value. Is this ok?
                        },
                        AuthState::NeedsRefresh => {
                            debug!("Auth: Refreshing auth.");
                            Ok(refresh_authentication(&auth_details).await?)
                        },
                        AuthState::NeedsReauthentication => {
                            debug!("Auth: Reacquiring auth.");
                            Ok(authenticate_credentials(credentials).await?)
                        }
                    }
                },
                Err(e) => Err(e)
            }
        }
    }
}

/*
 * Common function used when generating or refreshing.
 */
async fn authenticate(form_body: &HashMap<&str, String>) -> Result<AuthDetails, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let response: Response = client.post(AUTH_URL).form(form_body).send().await?;
    // Await the result of our auth request
    if response.status().is_success() {
        let body = response.text().await.unwrap();
        let mut auth_details: AuthDetails = serde_json::from_str(&body)?;
        auth_details.acquired_time = Utc::now().timestamp();
        Ok(auth_details)
    } else {
        // Debug ok here, since this is effectively a stop error
        Err(format!("authentication response was abnormal: {response:?}").into())
    }
}

/*
 * Credentials are required for a new auth object.
 */
pub async fn authenticate_credentials(credentials: &Credentials) -> Result<AuthDetails, Box<dyn Error>> {
    let form_body = if let (Some(user), Some(pass)) = (credentials.user.clone(), credentials.pass.clone()) {
        HashMap::from([
            ("client_id", String::from("cdse-public")),
            ("grant_type", String::from("password")),
            ("username", user),
            ("password", pass)
        ])
    } else {
        HashMap::new()
    };
    authenticate(&form_body).await
}

/*
 * Refreshing our auth requires slightly different headers from the from-scratch flow.
 */
pub async fn refresh_authentication(auth_details: &AuthDetails) -> Result<AuthDetails, Box<dyn Error>> {
    let form_body = HashMap::from([
        ("client_id", String::from("cdse-public")),
        ("grant_type", String::from("refresh_token")),
        ("refresh_token", auth_details.refresh_token.clone()),
    ]);
    authenticate(&form_body).await
}

// API Interactions

/*
 * Params for the search endpoints: List, Search
 */
#[derive(Debug, Default)]
pub struct QueryParams {
    pub ids: Option<String>,
    pub collections: Option<String>,
    pub bbox: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sortby: Option<String>,
    pub limit: Option<u16>,
    pub page: Option<u16>,
}

/*
 * impl to make converting from passed args to search params easy.
 */
impl From<SearchArgs> for QueryParams {
    fn from(a: SearchArgs) -> Self {
        let SearchArgs { ids, collections, bbox, from, to, sortby, limit, page, .. } = a;
        QueryParams { ids, collections, bbox, from, to, sortby, limit, page }
    }
}

impl From<DownloadArgs> for QueryParams {
    fn from(da: DownloadArgs) -> Self {
        QueryParams { ids: da.ids, ..Default::default() }
    }
}

/*
 * Generates query params from QueryParams
 * Return value matches interface provided by Url.set_query
 */
fn generate_query(
    query_params: QueryParams,
    include_collections: bool,
) -> Option<String> {
    let mut options: Vec<String> = Vec::new();

    if let Some(ids) = query_params.ids {
        options.push(format!("ids={ids}"));
    }

    if let Some(bbox) = query_params.bbox {
        options.push(format!("bbox={bbox}"));
    }

    if query_params.from.is_some() || query_params.to.is_some() {
        options.push(format!(
            "datetime={}/{}",
            if let Some(from) = query_params.from { from.to_rfc3339_opts(Secs, true) } else { String::from("") },
            if let Some(to) = query_params.to { to.to_rfc3339_opts(Secs, true) } else { String::from("") }
        ));
    }

    if let Some(sortby) = query_params.sortby {
        options.push(format!("sortby={sortby}"));
    }

    if let Some(limit) = query_params.limit {
        options.push(format!("limit={limit}"));
    }

    if let Some(page) = query_params.page {
        options.push(format!("page={page}"));
    }

    if include_collections {
        if let Some(collections) = query_params.collections {
            options.push(format!("collections={collections}"));
        }
    }

    if !options.is_empty() {
        Some(options.join("&"))
    } else {
        None
    }
}


/*
 * Gets some values from a response object: length, file details.
 */
fn get_header_info(r: &Response) -> (usize, String) {
    let h = r.headers();
    // Get header value, assume string and not bytes, then convert to usize.
    let length_value = if let Some(v) = h.get("content-length") { v.to_str() } else { Ok("0") };
    let length = if let Ok(l) = length_value { l.parse::<usize>().unwrap_or(0) } else { 0 };
    // Get header value, convert to strings, don't bother parsing them yet.
    let disposition_value = if let Some(v) = h.get("content-disposition") { v.to_str() } else { Ok("") };
    let full_disposition = if let Ok(dv) = disposition_value { String::from(dv) } else { String::new() };
    let disposition_file = full_disposition.split("filename=").last().unwrap_or("").to_string();

    (length, disposition_file)
}

/*
 * Composes a path and output file for downloads.
 */
fn compose_path(opt_path: Option<String>, name: &String) -> PathBuf {
    if let Some(path) = opt_path {
        [&path, &format!("{name}.zip")].iter().collect()
    } else {
        ["./", &format!("{name}.zip")].iter().collect()
    }
}

// Queries for imagery that satisfies constraints
pub async fn search_imagery(
    client: &Client,
    auth_details: &AuthDetails,
    query_params: QueryParams,
) -> Result<FeatureCollection, Box<dyn Error>> {
    let mut url: Url = Url::parse(SEARCH_URL)?;
    let query_params = generate_query(query_params, true);
    url.set_query(query_params.as_deref());

    info!("API::list_imagery: Requesting {url}...");
    let response_text = client
        .get(url)
        .header("Authorization", format!("Bearer {}", auth_details.access_token))
        .send().await.unwrap().text().await.unwrap_or(String::from("{}"));
    info!("API::list_imagery: Response: \n{response_text}");
    let geojson = response_text.parse::<GeoJson>()?;
    let fc: FeatureCollection = FeatureCollection::try_from(geojson)?;
    Ok(fc)
}

/*
 * Small output struct for conveying some download details to the caller.
 */
#[derive(Debug)]
pub struct DownloadDetails {
    pub destination: PathBuf,
    pub size: usize,
}

/*
 * Downloads imagery product for passed feature. The Copernicus Program's search
 * output takes the shape of features in a feature collection, which each feature
 * containing metadata that describes where to get its quicklook and product
 * bundle, i.e. imagery.
 */
pub async fn download_imagery(
    client: &Client,
    auth_details: &AuthDetails,
    feature: &Feature,
    output_dir: Option<String>,
) -> Result<DownloadDetails, Box<dyn Error>> {
    let feature_id = get_id(&feature.id);
    let path = Vec::from(["assets", "PRODUCT", "href"]);
    let product_url = get_value(from_path(path, &feature.foreign_members));
    if let (Some(id), Some(catalogue_href)) = (feature_id, product_url) {
        // This seems to be required by the API. The URI we obtain has the catalogue subdomain, and
        // when curl'ed or wget'ed the API responds with a 301 redirecting to the download
        // subdomain, but seemingly returns a 401s for this tool.
        // The Python example in the official docs begins with a download subdomain url, so it's
        // not clear whether it's expected that you do string substitution when using the feature's
        // product URL.
        let download_url = catalogue_href.replace("catalogue", "download");
        let url = Url::parse(&download_url)?;
        let request = client
            .get(url)
            .timeout(Duration::from_secs(1_000_000))
            .header("Authorization", format!("Bearer {}", auth_details.access_token));
        let response = request.send().await?;
        // Create file, write byte stream
        if response.status().is_success() {
            // Unused at the moment, but will let us show some extra info during downloads
            let (_length, _file) = get_header_info(&response);
            let path = compose_path(output_dir, &id);
            let mut f = File::create(&path)?;
            let mut stream = response.bytes_stream();
            let mut bytes_total: usize = 0;
            loop {
                if let Some(bytes) = stream.next().await {
                    match f.write(&bytes?) {
                        Ok(n) => {
                            debug!("wrote {n} bytes");
                            bytes_total += n;
                        },
                        Err(e) => {
                            error!("Something went wrong: {e}");
                            break;
                        }
                    }
                } else {
                    debug!("write ended.");
                    break;
                }
            }
            Ok(DownloadDetails {
                destination: path,
                size: bytes_total
            })
        } else {
            println!("failed. response:\n{response:#?}");
            Err(format!("Failure response from server: {response:#?}").into())
        }
    } else {
        Err(format!("Unable to download resource {:?}", feature.id).into())
    }
}


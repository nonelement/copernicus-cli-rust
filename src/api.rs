use std::collections::HashMap;
use std::convert::From;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::offset::Utc;
use chrono::{DateTime, SecondsFormat::Secs};
use futures_util::StreamExt;
use geojson::{Feature, FeatureCollection};
use log::{info, error};
use reqwest::{Client, Response};
use serde::{Serialize, Deserialize};
use url::Url;

use crate::Credentials;
use crate::args::Args;
use crate::util::{get_id, get_value, from_path};

// POST
const AUTH_URL: &str = "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";
// GET
const LIST_URL: &str = "https://catalogue.dataspace.copernicus.eu/stac/collections/{}/items";

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
                            info!("Auth: Existing auth ok, resuing.");
                            Ok(auth_details) // TODO: This returns the moved value. Is this ok?
                        },
                        AuthState::NeedsRefresh => {
                            info!("Auth: Refreshing auth.");
                            Ok(refresh_authentication(&auth_details).await?)
                        },
                        AuthState::NeedsReauthentication => {
                            info!("Auth: Reacquiring auth.");
                            Ok(authenticate_credentials(credentials).await?)
                        }
                    }
                },
                Err(e) => Err(e)
            }
        }
    }
}

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
        Err(format!("authentication response was abnormal: {:?}", response).into())
    }
}

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

pub async fn refresh_authentication(auth_details: &AuthDetails) -> Result<AuthDetails, Box<dyn Error>> {
    let form_body = HashMap::from([
        ("client_id", String::from("cdse-public")),
        ("grant_type", String::from("refresh_token")),
        ("refresh_token", auth_details.refresh_token.clone()),
    ]);
    authenticate(&form_body).await
}

// API Interactions

// Params for list endpoint. Most can be used together to filter results.
pub struct ListParams {
    pub ids: Option<String>,
    pub collection: Option<String>,
    pub bbox: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sortby: Option<String>,
    pub limit: Option<u16>,
    pub page: Option<u16>,
}

impl From<Args> for ListParams {
    fn from(a: Args) -> Self {
        let Args { ids, collection, bbox, from, to, sortby, limit, page, .. } = a;
        ListParams { ids, collection, bbox, from, to, sortby, limit, page }
    }
}

// Generates query params from ListParams
// Return value matches interface provided by Url.set_query
fn generate_query(
    list_params: ListParams
) -> Option<String> {
    let mut options: Vec<String> = Vec::new();

    if let Some(ids) = list_params.ids {
        options.push(format!("ids={}", ids));
    }

    if let Some(bbox) = list_params.bbox {
        options.push(format!("bbox={}", bbox));
    }

    if list_params.from.is_some() || list_params.to.is_some() {
        options.push(format!(
            "datetime={}/{}",
            if let Some(from) = list_params.from { from.to_rfc3339_opts(Secs, true) } else { String::from("") },
            if let Some(to) = list_params.to { to.to_rfc3339_opts(Secs, true) } else { String::from("") }
        ));
    }

    if let Some(sortby) = list_params.sortby {
        options.push(format!("sortby={}", sortby));
    }

    if let Some(limit) = list_params.limit {
        options.push(format!("limit={}", limit));
    }

    if let Some(page) = list_params.page {
        options.push(format!("page={}", page));
    }

    if !options.is_empty() {
        Some(options.join("&"))
    } else {
        None
    }
}

fn with_collection(url: &'static str, collection: &Option<String>) -> Result<String, Box<dyn Error>> {
    // Can be used as a template
    if let Some(collection_id) = collection {
        if url.contains("{}") {
            Ok(url.replace("{}", collection_id))
        } else {
            Err("Unable to use provided url as a template.".into())
        }
    } else {
        Err("No collection provided.".into())
    }
}

// Queries for imagery that satisfies constraints
pub async fn list_imagery(
    client: &Client,
    auth_details: &AuthDetails,
    list_params: ListParams,
) -> Result<FeatureCollection, Box<dyn Error>> {
    let mut url: Url = Url::parse(&with_collection(LIST_URL, &list_params.collection)?)?;
    let query_params = generate_query(list_params);
    url.set_query(query_params.as_deref());

    info!("API::list_imagery: Requesting {}...", url);
    let response_text = client
        .get(url)
        .header("Authorization", format!("Bearer {}", auth_details.access_token))
        .send().await.unwrap().text().await.unwrap_or(String::from("{}"));
    info!("API::list_imagery: Response: \n{}", response_text);
    let maybe_fc = serde_json::from_str::<FeatureCollection>(&response_text);
    match maybe_fc {
        Ok(fc) => Ok(fc),
        Err(e) => {
            error!("Unable to deserialize response: {}.\nResponse:{}", e, response_text);
            Err(Box::new(e))
        }
    }
}

#[derive(Debug)]
pub struct DownloadDetails {
    pub ids: Vec<String>,
    pub url: String,
    pub destination: String,
    pub size: u64,
}

// URL example: https://catalogue.dataspace.copernicus.eu/odata/v1/Products(56db10b0-ede4-4332-a110-2a6ae003048a)/$value
pub async fn download_imagery(
    client: &Client,
    auth_details: &AuthDetails,
    feature: &Feature,
) -> Result<DownloadDetails, Box<dyn Error>> {
    let feature_id = get_id(&feature.id);
    let product_url = get_value(from_path(vec!["assets", "PRODUCT", "href"], &feature.foreign_members));
    if let (Some(id), Some(href)) = (feature_id, product_url) {
        let url = Url::parse(&href)?;
        let request = client
            .get(url)
            .timeout(Duration::from_secs(1_000_000))
            .header("Authorization", format!("Bearer {}", auth_details.access_token));
        println!("request:\n{:#?}", request);
        let response = request.send().await?;
        // Create file, write byte stream
        if response.status().is_success() {
            let mut f = File::create(format!("{id}.zip"))?;
            let mut stream = response.bytes_stream();
            loop {
                if let Some(bytes) = stream.next().await {
                    match f.write(&bytes?) {
                        Ok(n) => println!("wrote {} bytes", n),
                        Err(_) => {
                            println!("something went wrong!");
                            break;
                        }
                    }
                } else {
                    println!("done writing!");
                    break;
                }
            }
        } else {
            println!("failed. response:\n{:#?}", response);
        }

        // Write bytes here.
        Ok(DownloadDetails { ids: vec![], url: String::new(), destination: String::new(), size: 0 })
    } else {
        Err(format!("Unable to download resource {:?}", feature.id).into())
    }
}


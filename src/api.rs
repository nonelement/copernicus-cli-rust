use std::collections::HashMap;
use std::convert::From;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::offset::Utc;
use chrono::{DateTime, SecondsFormat::Secs};
use geojson::FeatureCollection;
use log::{info, error};
use reqwest::{Client, Response};
use serde::{Serialize, Deserialize};
use url::Url;

use crate::Args;

// Related to both CLI ENV and Auth interactions
pub struct Credentials {
    pub user: Option<String>,
    pub pass: Option<String>
}

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
// TODO: implement search endpoint to search multiple catalogues?
//  e.g. Sentinel-1, Sentinel-2, Sentinel-3, etc.

fn with_collection(url: &'static str, collection: &str) -> Result<String, Box<dyn Error>> {
    // Can be used as a template
    if url.contains("{}") {
        Ok(url.replace("{}", collection))
    } else {
        Err("Unable to use provided url as a template.".into())
    }
}

// Params for list endpoint. Most can be used together to filter results.
pub struct ListParams {
    pub id: Option<String>,
    pub collection: String,
    pub bbox: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sortby: Option<String>,
    pub limit: Option<u16>,
    pub page: Option<u16>,
}

impl From<Args> for ListParams {
    fn from(a: Args) -> Self {
        let Args { id, collection, bbox, from, to, sortby, limit, page } = a;
        ListParams { id, collection, bbox, from, to, sortby, limit, page }
    }
}

// Generates query params from ListParams
// Return value matches interface provided by Url.set_query
fn generate_query(
    list_params: ListParams
) -> Option<String> {
    let mut options: Vec<String> = Vec::new();

    if let Some(id) = list_params.id {
        options.push(format!("ids={}", id));
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


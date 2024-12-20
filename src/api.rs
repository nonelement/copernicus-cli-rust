use std::collections::HashMap;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use geojson::FeatureCollection;
use serde::{Serialize, Deserialize};
use reqwest::{Client, Response};
use log::info;

pub struct Credentials {
    pub user: Option<String>,
    pub pass: Option<String>
}

// POST
const AUTH_URL: &str = "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";
// GET
const LIST_URL: &str = "https://catalogue.dataspace.copernicus.eu/stac/collections/SENTINEL-2/items?bbox=<BBOX>";


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthDetails {
    #[serde(default)]
    pub acquired_time: u64, // When authentication was acquired, to check current age
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

enum AuthState {
    IsOK,
    NeedsRefresh,
    NeedsReauthentication,
}

fn get_auth_state(auth_details: &AuthDetails) -> Result<AuthState, Box<dyn Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let is_expired = now > (auth_details.acquired_time + auth_details.expires_in as u64).try_into()?;
    let is_refresh_expired = now > (auth_details.acquired_time + auth_details.refresh_expires_in as u64).try_into()?;
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
            return Ok(authenticate_credentials(credentials).await?)
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
        auth_details.acquired_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        return Ok(auth_details);
    } else {
        // Debug ok here, since this is effectively a stop error
        return Err(format!("authentication response was abnormal: {:?}", response).into());
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
    Ok(authenticate(&form_body).await?)
}

pub async fn refresh_authentication(auth_details: &AuthDetails) -> Result<AuthDetails, Box<dyn Error>> {
    let form_body = HashMap::from([
        ("client_id", String::from("cdse-public")),
        ("grant_type", String::from("refresh_token")),
        ("refresh_token", String::from(auth_details.refresh_token.clone())),
    ]);
    Ok(authenticate(&form_body).await?)
}

pub async fn list_imagery(client: &Client, auth_details: &AuthDetails, bbox: String) -> Result<FeatureCollection, Box<dyn Error>> {
    let clean_bbox = &bbox[1..bbox.len()-1];
    let url = LIST_URL.replace("<BBOX>", clean_bbox);
    info!("API::list_imagery: Requesting {}...", url);
    let response_text = client
        .get(url)
        .header("Authorization", format!("Bearer {}", auth_details.access_token))
        .send().await.unwrap().text().await.unwrap_or(String::from("{}"));
    return Ok(serde_json::from_str::<FeatureCollection>(&response_text)?);
}

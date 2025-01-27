#![allow(non_snake_case)]

#[cfg(feature = "enterprise_saml")]
use axum::response::Redirect;
use axum::Json;
use axum::{routing::post, Router};
#[cfg(feature = "enterprise_saml")]
use axum::{Extension, Form};
use std::sync::Arc;

#[cfg(feature = "enterprise_saml")]
use samael::metadata::{ContactPerson, ContactType, EntityDescriptor};
#[cfg(feature = "enterprise_saml")]
use samael::service_provider::{ServiceProvider, ServiceProviderBuilder};

#[cfg(feature = "enterprise_saml")]
use serde::Deserialize;
#[cfg(feature = "enterprise_saml")]
use tower_cookies::Cookies;
#[cfg(feature = "enterprise_saml")]
use windmill_common::error::{Error, Result};

#[cfg(feature = "enterprise_saml")]
use crate::db::DB;
#[cfg(feature = "enterprise_saml")]
use crate::oauth2_ee::login_externally;
#[cfg(feature = "enterprise_saml")]
use crate::BASE_URL;

#[cfg(feature = "enterprise_saml")]
use windmill_common::ee::{get_license_plan, LicensePlan};

#[cfg(feature = "enterprise_saml")]
#[derive(Clone)]
pub struct ServiceProviderExt(pub Option<ServiceProvider>);

#[cfg(not(feature = "enterprise_saml"))]
pub struct ServiceProviderExt();

#[cfg(not(feature = "enterprise_saml"))]
pub async fn build_sp_extension() -> anyhow::Result<ServiceProviderExt> {
    return Ok(ServiceProviderExt());
}

#[cfg(feature = "enterprise_saml")]
pub async fn build_sp_extension() -> anyhow::Result<ServiceProviderExt> {
    use crate::SAML_METADATA;

    if !matches!(get_license_plan().await, LicensePlan::Enterprise) {
        return Ok(ServiceProviderExt(None));
    }
    if let Some(url_metadata) = SAML_METADATA.read().await.clone() {
        let idp_metadata: EntityDescriptor = load_metadata(url_metadata.as_str()).await?;

        // let pub_key = openssl::x509::X509::from_pem("")?;
        // let private_key = openssl::rsa::Rsa::private_key_from_pem("")?;

        let acs_url = format!("{}/api/saml/acs", BASE_URL.read().await.clone());
        let sp = ServiceProviderBuilder::default()
            .entity_id("windmill".to_string())
            // .key(private_key)
            // .certificate(pub_key)
            .allow_idp_initiated(true)
            .contact_person(ContactPerson {
                sur_name: Some("Ruben Fiszel <ruben@windmill.dev>".to_string()),
                contact_type: Some(ContactType::Technical.value().to_string()),
                ..ContactPerson::default()
            })
            .idp_metadata(idp_metadata)
            .acs_url(acs_url.clone())
            .build()?;

        tracing::info!("SAML Configured - ACS url is {}", acs_url);
        Ok(ServiceProviderExt(Some(sp)))
    } else {
        Ok(ServiceProviderExt(None))
    }
}

#[cfg(not(feature = "enterprise_saml"))]
pub async fn generate_redirect_url(
    _service_provider: Arc<ServiceProviderExt>,
) -> anyhow::Result<Option<String>> {
    return Ok(None);
}

#[cfg(feature = "enterprise_saml")]
pub async fn generate_redirect_url(
    service_provider: Arc<ServiceProviderExt>,
) -> anyhow::Result<Option<String>> {
    if let Some(sp) = &service_provider.0 {
        let url = sp
            .idp_metadata
            .idp_sso_descriptors
            .clone()
            .unwrap_or_default()
            .get(0)
            .and_then(|x| x.single_sign_on_services.get(0).map(|x| x.location.clone()));

        let authn_req = sp
            .make_authentication_request(url.unwrap_or_default().as_str())
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let redirect_url = authn_req
            .redirect(BASE_URL.read().await.clone().as_str())
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .map(|u| u.to_string());

        tracing::debug!(
            "SAML Configured, sso login link at: {:?}",
            redirect_url.clone()
        );
        Ok(redirect_url)
    } else {
        Ok(None)
    }
}

#[cfg(feature = "enterprise_saml")]
async fn load_metadata(saml_metadata: &str) -> anyhow::Result<EntityDescriptor> {
    let resp = if saml_metadata.starts_with("http") {
        reqwest::get(saml_metadata).await?.text().await?
    } else {
        saml_metadata.to_string()
    };
    let idp_metadata: EntityDescriptor = samael::metadata::de::from_str(&resp).map_err(|e| {
        Error::BadRequest(format!(
            "SAML metadata could not be decoded: {e}. Content was: {resp}"
        ))
    })?;
    return Ok(idp_metadata);
}
#[cfg(feature = "enterprise_saml")]
pub async fn test_metadata(Json(saml_metadata): Json<String>) -> Result<String> {
    let idp_metadata = load_metadata(&saml_metadata).await?;
    Ok(format!("{:?}", idp_metadata))
}

#[cfg(feature = "enterprise_saml")]
pub fn global_service() -> Router {
    Router::new()
        .route("/acs", post(acs))
        .route("/test_metadata", post(test_metadata))
}

#[cfg(not(feature = "enterprise_saml"))]
pub fn global_service() -> Router {
    Router::new()
}

#[cfg(feature = "enterprise_saml")]
#[derive(Deserialize)]
pub struct SamlForm {
    pub SAMLResponse: Option<String>,
}

#[cfg(feature = "enterprise_saml")]
pub async fn acs(
    Extension(db): Extension<DB>,
    cookies: Cookies,
    Extension(se): Extension<Arc<ServiceProviderExt>>,
    Form(s): Form<SamlForm>,
) -> Result<Redirect> {
    if matches!(get_license_plan().await, LicensePlan::Pro) {
        return Err(Error::BadRequest(
            "SAML not available in the pro plan".to_string(),
        ));
    };
    if let Some(sp_m) = &se.0 {
        let sp = sp_m.clone();
        if let Some(encoded_resp) = s.SAMLResponse {
            tracing::info!("{:?}", encoded_resp);
            let t = sp.parse_base64_response(&encoded_resp, None).map_err(|e| {
                Error::BadRequest(format!("Error parsing acs request as base64: {e:?}"))
            })?;

            if let Some(email) = t.subject.and_then(|x| x.name_id.map(|x| x.value)) {
                login_externally(db, &email, "saml".to_string(), cookies, None, None).await?;
                Ok(Redirect::to("/"))
            } else {
                Err(Error::BadRequest(
                    "email not found in saml response".to_string(),
                ))
            }
        } else {
            Err(Error::BadRequest("SAMLResponse not found".to_string()))
        }
    } else {
        Err(Error::BadConfig("SAML not configured".to_string()))
    }
}

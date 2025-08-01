use std::collections::{
    hash_map::Entry::{Occupied, Vacant},
    HashMap,
};

use crate::{
    db::{ApiAuthed, DB},
    postgres_triggers::mapper::{Mapper, MappingInfo},
    utils::check_scopes,
};
use axum::{
    extract::{Path, Query},
    Extension, Json,
};
use http::StatusCode;
use itertools::Itertools;
use pg_escape::{quote_identifier, quote_literal};
use quick_cache::sync::Cache;
use rust_postgres::{types::Type, Client};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use sql_builder::{bind::Bind, SqlBuilder};
use sqlx::FromRow;
use windmill_audit::{audit_oss::audit_log, ActionKind};
use windmill_common::{
    db::UserDB,
    error::{self, to_anyhow, Error, JsonResult, Result},
    utils::{empty_as_none, not_found_if_none, paginate, Pagination, StripPath},
    worker::CLOUD_HOSTED,
};
use windmill_git_sync::{handle_deployment_metadata, DeployedObject};

use super::{
    check_if_valid_publication_for_postgres_version, create_logical_replication_slot,
    create_pg_publication, drop_publication, generate_random_string, get_default_pg_connection,
    ERROR_PUBLICATION_NAME_NOT_EXISTS,
};
use anyhow::anyhow;
use lazy_static::lazy_static;

#[derive(FromRow, Serialize, Deserialize, Debug)]
pub struct Postgres {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: Option<u16>,
    pub dbname: String,
    #[serde(default)]
    pub sslmode: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub root_certificate_pem: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TableToTrack {
    pub table_name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub where_clause: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub columns_name: Option<Vec<String>>,
}

impl TableToTrack {
    fn new(
        table_name: String,
        where_clause: Option<String>,
        columns_name: Option<Vec<String>>,
    ) -> TableToTrack {
        TableToTrack { table_name, where_clause, columns_name }
    }
}

lazy_static! {
    pub static ref TEMPLATE: Cache<String, String> = Cache::new(50);
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Relations {
    pub schema_name: String,
    pub table_to_track: Vec<TableToTrack>,
}

impl Relations {
    fn new(schema_name: String, table_to_track: Vec<TableToTrack>) -> Relations {
        Relations { schema_name, table_to_track }
    }

    fn add_new_table(&mut self, table_to_track: TableToTrack) {
        self.table_to_track.push(table_to_track);
    }
}

#[derive(Debug, Deserialize)]
pub struct EditPostgresTrigger {
    replication_slot_name: String,
    publication_name: String,
    path: String,
    script_path: String,
    is_flow: bool,
    postgres_resource_path: String,
    publication: Option<PublicationData>,
    error_handler_path: Option<String>,
    error_handler_args: Option<sqlx::types::Json<HashMap<String, Box<RawValue>>>>,
    retry: Option<sqlx::types::Json<windmill_common::flows::Retry>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct NewPostgresTrigger {
    path: String,
    script_path: String,
    is_flow: bool,
    enabled: bool,
    postgres_resource_path: String,
    replication_slot_name: Option<String>,
    publication_name: Option<String>,
    publication: Option<PublicationData>,
    error_handler_path: Option<String>,
    error_handler_args: Option<sqlx::types::Json<HashMap<String, Box<RawValue>>>>,
    retry: Option<sqlx::types::Json<windmill_common::flows::Retry>>,
}

#[derive(Serialize, Deserialize)]
pub struct TestPostgres {
    pub postgres_resource_path: String,
}

pub async fn test_postgres_connection(
    authed: ApiAuthed,
    Extension(db): Extension<DB>,
    Extension(user_db): Extension<UserDB>,
    Path(workspace_id): Path<String>,
    Json(test_postgres): Json<TestPostgres>,
) -> Result<()> {
    let connect_f = async {
        get_default_pg_connection(
            authed,
            Some(user_db),
            &db,
            &test_postgres.postgres_resource_path,
            &workspace_id,
        )
        .await
        .map_err(|err| {
            error::Error::BadConfig(format!("Error connecting to postgres: {}", err.to_string()))
        })
    };
    tokio::time::timeout(tokio::time::Duration::from_secs(30), connect_f)
        .await
        .map_err(|_| {
            error::Error::BadConfig(format!("Timeout connecting to postgres after 30 seconds"))
        })??;

    Ok(())
}

#[derive(Deserialize, Debug)]
pub enum Language {
    #[serde(rename = "typescript", alias = "Typescript")]
    Typescript,
}

#[derive(Debug, Deserialize)]
pub struct TemplateScript {
    postgres_resource_path: String,
    #[serde(deserialize_with = "check_if_valid_relation")]
    relations: Option<Vec<Relations>>,
    language: Language,
}

fn check_if_valid_relation<'de, D>(
    relations: D,
) -> std::result::Result<Option<Vec<Relations>>, D::Error>
where
    D: Deserializer<'de>,
{
    let relations: Option<Vec<Relations>> = Option::deserialize(relations)?;
    let mut track_all_table_in_schema = false;
    let mut track_specific_columns_in_table = false;
    match relations {
        Some(relations) => {
            for relation in relations.iter() {
                if relation.schema_name.is_empty() {
                    return Err(serde::de::Error::custom(
                        "Schema Name must not be empty".to_string(),
                    ));
                }

                if !track_all_table_in_schema && relation.table_to_track.is_empty() {
                    track_all_table_in_schema = true;
                    continue;
                }

                for table_to_track in relation.table_to_track.iter() {
                    if table_to_track.table_name.trim().is_empty() {
                        return Err(serde::de::Error::custom(
                            "Table name must not be empty".to_string(),
                        ));
                    }

                    if !track_specific_columns_in_table && table_to_track.columns_name.is_some() {
                        track_specific_columns_in_table = true;
                    }
                }

                if track_all_table_in_schema && track_specific_columns_in_table {
                    return Err(serde::de::Error::custom("Incompatible tracking options. Schema-level tracking and specific table tracking with column selection cannot be used together. Refer to the documentation for valid configurations."));
                }
            }

            if !relations
                .iter()
                .map(|relation| relation.schema_name.as_str())
                .all_unique()
            {
                return Err(serde::de::Error::custom(
                    "You cannot choose a schema more than one time".to_string(),
                ));
            }

            Ok(Some(relations))
        }
        None => Ok(None),
    }
}

#[derive(FromRow, Deserialize, Serialize, Debug)]
pub struct PostgresTrigger {
    pub path: String,
    pub script_path: String,
    pub is_flow: bool,
    pub workspace_id: String,
    pub edited_by: String,
    pub email: String,
    pub edited_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_perms: Option<serde_json::Value>,
    pub postgres_resource_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub replication_slot_name: String,
    pub publication_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_server_ping: Option<chrono::DateTime<chrono::Utc>>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handler_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handler_args: Option<sqlx::types::Json<HashMap<String, Box<RawValue>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<sqlx::types::Json<windmill_common::flows::Retry>>,
}

#[derive(Deserialize, Serialize)]
pub struct ListPostgresTriggerQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub path: Option<String>,
    pub is_flow: Option<bool>,
    pub path_start: Option<String>,
}

#[derive(Deserialize)]
pub struct SetEnabled {
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct PostgresPublicationReplication {
    publication_name: String,
    replication_slot_name: String,
}

impl PostgresPublicationReplication {
    pub fn new(
        publication_name: String,
        replication_slot_name: String,
    ) -> PostgresPublicationReplication {
        PostgresPublicationReplication { publication_name, replication_slot_name }
    }
}

async fn check_if_logical_replication_slot_exist(
    pg_connection: &mut Client,
    replication_slot_name: &str,
) -> Result<bool> {
    let row = pg_connection
        .query_opt(
            "SELECT slot_name FROM pg_replication_slots WHERE slot_name = $1",
            &[&replication_slot_name],
        )
        .await
        .map_err(to_anyhow)?;
    Ok(row.is_some())
}

async fn create_custom_slot_and_publication_inner(
    authed: ApiAuthed,
    user_db: UserDB,
    db: &DB,
    postgres_resource_path: &str,
    w_id: &str,
    publication: &PublicationData,
) -> Result<PostgresPublicationReplication> {
    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let tx = pg_connection.transaction().await.map_err(to_anyhow)?;

    let publication_name = format!("windmill_trigger_{}", generate_random_string());
    let replication_slot_name = publication_name.clone();

    create_logical_replication_slot(tx.client(), &replication_slot_name).await?;
    create_pg_publication(
        &tx.client(),
        &publication_name,
        publication.table_to_track.as_deref(),
        &publication.transaction_to_track,
    )
    .await?;

    tx.commit().await.map_err(to_anyhow)?;

    Ok(PostgresPublicationReplication::new(
        publication_name,
        replication_slot_name,
    ))
}

pub async fn get_postgres_version_internal(pg_connection: &Client) -> Result<String> {
    let row = pg_connection
        .query_one("SHOW server_version;", &[])
        .await
        .map_err(to_anyhow)?;

    let postgres_version: String = row.get(0);

    Ok(postgres_version)
}

pub async fn get_postgres_version(
    authed: ApiAuthed,
    Extension(db): Extension<DB>,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
) -> Result<String> {
    let pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let postgres_version = get_postgres_version_internal(&pg_connection).await?;

    Ok(postgres_version)
}

pub async fn create_postgres_trigger(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path(w_id): Path<String>,
    Json(new_postgres_trigger): Json<NewPostgresTrigger>,
) -> Result<(StatusCode, String)> {
    check_scopes(&authed, || format!("postgres_triggers:write:{}", new_postgres_trigger.path))?;

    if *CLOUD_HOSTED {
        return Err(error::Error::BadRequest(
            "Postgres triggers are not supported on multi-tenant cloud, use dedicated cloud or self-host".to_string(),
        ));
    }

    let NewPostgresTrigger {
        postgres_resource_path,
        path,
        script_path,
        enabled,
        is_flow,
        publication_name,
        replication_slot_name,
        publication,
        error_handler_path,
        error_handler_args,
        retry,
    } = new_postgres_trigger;

    if publication_name.is_none() && publication.is_none() {
        return Err(error::Error::BadRequest(
            "Publication data is missing".to_string(),
        ));
    }
    let (pub_name, slot_name) = if publication_name.is_none() && replication_slot_name.is_none() {
        if publication.is_none() {
            return Err(Error::BadRequest("publication must be set".to_string()));
        }

        let PostgresPublicationReplication { publication_name, replication_slot_name } =
            create_custom_slot_and_publication_inner(
                authed.clone(),
                user_db.clone(),
                &db,
                &postgres_resource_path,
                &w_id,
                &publication.unwrap(),
            )
            .await?;

        (publication_name, replication_slot_name)
    } else {
        if publication_name.is_none() {
            return Err(Error::BadRequest("Missing publication name".to_string()));
        } else if replication_slot_name.is_none() {
            return Err(Error::BadRequest(
                "Missing replication slot name".to_string(),
            ));
        }
        (publication_name.unwrap(), replication_slot_name.unwrap())
    };

    let mut tx = user_db.begin(&authed).await?;

    sqlx::query!(
        r#"
        INSERT INTO postgres_trigger (
            publication_name,
            replication_slot_name,
            workspace_id, 
            path, 
            script_path, 
            is_flow, 
            email, 
            enabled, 
            postgres_resource_path, 
            edited_by,
            error_handler_path,
            error_handler_args,
            retry
        ) 
        VALUES (
            $1, 
            $2, 
            $3, 
            $4, 
            $5, 
            $6, 
            $7, 
            $8, 
            $9, 
            $10,
            $11,
            $12,
            $13
        )"#,
        pub_name,
        slot_name,
        &w_id,
        &path,
        script_path,
        is_flow,
        &authed.email,
        enabled,
        postgres_resource_path,
        &authed.username,
        error_handler_path,
        error_handler_args as _,
        retry as _
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "postgres_triggers.create",
        ActionKind::Create,
        &w_id,
        Some(path.as_str()),
        None,
    )
    .await?;

    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::PostgresTrigger { path: path.to_string() },
        Some(format!("Postgres trigger '{}' created", path)),
        true,
    )
    .await?;

    Ok((StatusCode::CREATED, path.to_string()))
}

pub async fn list_postgres_triggers(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path(w_id): Path<String>,
    Query(lst): Query<ListPostgresTriggerQuery>,
) -> error::JsonResult<Vec<PostgresTrigger>> {
    let mut tx = user_db.begin(&authed).await?;
    let (per_page, offset) = paginate(Pagination { per_page: lst.per_page, page: lst.page });
    let mut sqlb = SqlBuilder::select_from("postgres_trigger")
        .fields(&[
            "workspace_id",
            "path",
            "script_path",
            "is_flow",
            "edited_by",
            "email",
            "edited_at",
            "server_id",
            "last_server_ping",
            "extra_perms",
            "error",
            "enabled",
            "postgres_resource_path",
            "replication_slot_name",
            "publication_name",
            "error_handler_path",
            "error_handler_args",
            "retry",
        ])
        .order_by("edited_at", true)
        .and_where("workspace_id = ?".bind(&w_id))
        .offset(offset)
        .limit(per_page)
        .clone();
    if let Some(path) = lst.path {
        sqlb.and_where_eq("script_path", "?".bind(&path));
    }
    if let Some(is_flow) = lst.is_flow {
        sqlb.and_where_eq("is_flow", "?".bind(&is_flow));
    }
    if let Some(path_start) = &lst.path_start {
        sqlb.and_where_like_left("path", path_start);
    }
    let sql = sqlb
        .sql()
        .map_err(|e| error::Error::InternalErr(e.to_string()))?;
    let rows = sqlx::query_as::<_, PostgresTrigger>(&sql)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            tracing::debug!("Error fetching postgres_trigger: {:#?}", e);
            windmill_common::error::Error::InternalErr("server error".to_string())
        })?;
    tx.commit().await.map_err(|e| {
        tracing::debug!("Error commiting postgres_trigger: {:#?}", e);
        windmill_common::error::Error::InternalErr("server error".to_string())
    })?;

    Ok(Json(rows))
}

#[derive(Deserialize, Serialize, Debug)]
pub struct PublicationData {
    #[serde(default, deserialize_with = "check_if_valid_relation")]
    pub table_to_track: Option<Vec<Relations>>,
    #[serde(deserialize_with = "check_if_valid_transaction_type")]
    pub transaction_to_track: Vec<String>,
}

fn check_if_valid_transaction_type<'de, D>(
    transaction_type: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut transaction_type: Vec<String> = Vec::deserialize(transaction_type)?;
    if transaction_type.len() > 3 {
        return Err(serde::de::Error::custom(
            "More than 3 transaction type which is not authorized, you are only allowed to those 3 transaction types: Insert, Update and Delete"
                .to_string(),
        ));
    }
    transaction_type.sort_unstable();
    transaction_type.dedup();

    for transaction in transaction_type.iter() {
        match transaction.to_lowercase().as_ref() {
            "insert" => {},
            "update" => {},
            "delete" => {},
            _ => {
                return Err(serde::de::Error::custom(
                    "Only the following transaction types are allowed: Insert, Update and Delete (case insensitive)"
                        .to_string(),
                ))
            }
        }
    }

    Ok(transaction_type)
}

impl PublicationData {
    fn new(
        table_to_track: Option<Vec<Relations>>,
        transaction_to_track: Vec<String>,
    ) -> PublicationData {
        PublicationData { table_to_track, transaction_to_track }
    }
}

#[derive(FromRow, Debug, Serialize)]
pub struct SlotList {
    slot_name: Option<String>,
    active: Option<bool>,
}

pub async fn list_slot_name(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
) -> Result<Json<Vec<SlotList>>> {
    let pg_connection: Client = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let rows = pg_connection
        .query(
            r#"
            SELECT 
                slot_name,
                active
            FROM
                pg_replication_slots 
            WHERE 
                plugin = 'pgoutput' AND
                slot_type = 'logical';
            "#,
            &[],
        )
        .await
        .map_err(to_anyhow)?;

    let slots = rows
        .into_iter()
        .map(|row| SlotList { slot_name: row.get("slot_name"), active: row.get("active") })
        .collect();

    Ok(Json(slots))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Slot {
    name: String,
}

pub async fn create_slot(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
    Json(Slot { name }): Json<Slot>,
) -> Result<String> {
    let pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    create_logical_replication_slot(&pg_connection, &name).await?;

    Ok(format!("Replication slot {} created!", name))
}

pub async fn drop_logical_replication_slot(pg_connection: &Client, slot_name: &str) -> Result<()> {
    let row = pg_connection
        .query_opt(
            r#"
            SELECT 
                active_pid 
            FROM 
                pg_replication_slots 
            WHERE 
                slot_name = $1
            "#,
            &[&slot_name],
        )
        .await
        .map_err(to_anyhow)?;

    let active_pid = row.map(|r| r.get::<_, Option<i32>>(0)).flatten();

    if let Some(pid) = active_pid {
        pg_connection
            .execute("SELECT pg_terminate_backend($1)", &[&pid])
            .await
            .map_err(to_anyhow)?;
    }

    pg_connection
        .execute("SELECT pg_drop_replication_slot($1)", &[&slot_name])
        .await
        .map_err(to_anyhow)?;

    Ok(())
}

pub async fn drop_slot_name(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
    Json(Slot { name }): Json<Slot>,
) -> Result<String> {
    let pg_connection =
        get_default_pg_connection(authed, Some(user_db), &db, &postgres_resource_path, &w_id)
            .await
            .map_err(to_anyhow)?;

    drop_logical_replication_slot(&pg_connection, &name)
        .await
        .map_err(to_anyhow)?;

    Ok(format!("Replication slot {} deleted!", name))
}

pub async fn list_database_publication(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
) -> Result<Json<Vec<String>>> {
    let pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let rows = pg_connection
        .query(
            "SELECT pubname AS publication_name FROM pg_publication;",
            &[],
        )
        .await
        .map_err(to_anyhow)?;

    let publications = rows
        .into_iter()
        .map(|row| row.get::<_, String>("publication_name"))
        .collect_vec();

    Ok(Json(publications))
}

pub async fn get_publication_info(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, publication_name, postgres_resource_path)): Path<(String, String, String)>,
) -> Result<Json<PublicationData>> {
    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let publication_data =
        get_publication_scope_and_transaction(&mut pg_connection, &publication_name).await;

    let (all_table, transaction_to_track) = match publication_data {
        Ok(Some(pub_data)) => pub_data,
        Ok(None) => {
            return Err(Error::NotFound(
                ERROR_PUBLICATION_NAME_NOT_EXISTS.to_string(),
            ))
        }
        Err(e) => return Err(e),
    };

    let table_to_track = if !all_table {
        Some(get_tracked_relations(&mut pg_connection, &publication_name).await?)
    } else {
        None
    };
    Ok(Json(PublicationData::new(
        table_to_track,
        transaction_to_track,
    )))
}

pub async fn create_publication(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, publication_name, postgres_resource_path)): Path<(String, String, String)>,
    Json(publication_data): Json<PublicationData>,
) -> Result<String> {
    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let PublicationData { table_to_track, transaction_to_track } = publication_data;

    let tx = pg_connection.transaction().await.map_err(to_anyhow)?;

    create_pg_publication(
        tx.client(),
        &publication_name,
        table_to_track.as_deref(),
        &transaction_to_track,
    )
    .await?;

    tx.commit().await.map_err(to_anyhow)?;

    Ok(format!(
        "Publication {} successfully created!",
        publication_name
    ))
}

pub async fn delete_publication(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, publication_name, postgres_resource_path)): Path<(String, String, String)>,
) -> Result<String> {
    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    drop_publication(&mut pg_connection, &publication_name).await?;

    Ok(format!(
        "Publication {} successfully deleted!",
        publication_name
    ))
}

pub async fn update_pg_publication(
    pg_connection: &Client,
    publication_name: &str,
    PublicationData { table_to_track, transaction_to_track }: PublicationData,
    all_table: Option<bool>,
) -> Result<()> {
    let quoted_publication_name = quote_identifier(publication_name);
    let transaction_to_track_as_str = transaction_to_track.iter().join(",");
    match table_to_track {
        Some(ref relations) if !relations.is_empty() => {
            // If all_table is None, the publication does not exist yet
            if all_table.unwrap_or(true) {
                if all_table.is_some_and(|all_table| all_table) {
                    drop_publication(pg_connection, publication_name)
                        .await
                        .map_err(to_anyhow)?;
                }
                create_pg_publication(
                    pg_connection,
                    publication_name,
                    table_to_track.as_deref(),
                    &transaction_to_track,
                )
                .await
                .map_err(to_anyhow)?;
            } else {
                let pg_14 = check_if_valid_publication_for_postgres_version(
                    pg_connection,
                    table_to_track.as_deref(),
                )
                .await
                .map_err(to_anyhow)?;

                let mut query = format!("ALTER PUBLICATION {} SET ", quoted_publication_name);
                let mut first = true;

                for (i, schema) in relations.iter().enumerate() {
                    if schema.table_to_track.is_empty() {
                        query.push_str("TABLES IN SCHEMA ");
                        query.push_str(&quote_identifier(&schema.schema_name));
                    } else {
                        if pg_14 && first {
                            query.push_str("TABLE ONLY ");
                            first = false;
                        } else if !pg_14 {
                            query.push_str("TABLE ONLY ");
                        }

                        for (j, table) in schema.table_to_track.iter().enumerate() {
                            let table_name = quote_identifier(&table.table_name);
                            let schema_name = quote_identifier(&schema.schema_name);
                            let full_name = format!("{}.{}", schema_name, table_name);
                            query.push_str(&full_name);

                            if let Some(columns) = table.columns_name.as_ref() {
                                let cols =
                                    columns.iter().map(|col| quote_identifier(col)).join(", ");
                                query.push_str(&format!(" ({})", cols));
                            }

                            if let Some(where_clause) = &table.where_clause {
                                query.push_str(&format!(" WHERE ({})", where_clause));
                            }

                            if j + 1 != schema.table_to_track.len() {
                                query.push_str(", ");
                            }
                        }
                    }

                    if i + 1 != relations.len() {
                        query.push_str(", ");
                    }
                }

                pg_connection
                    .execute(&query, &[])
                    .await
                    .map_err(to_anyhow)?;

                let publish_query = format!(
                    "ALTER PUBLICATION {} SET (publish = '{}');",
                    quoted_publication_name, transaction_to_track_as_str
                );
                pg_connection
                    .execute(&publish_query, &[])
                    .await
                    .map_err(to_anyhow)?;
            }
        }
        _ => {
            drop_publication(pg_connection, publication_name)
                .await
                .map_err(to_anyhow)?;
            let create_all_query = format!(
                "CREATE PUBLICATION {} FOR ALL TABLES WITH (publish = '{}');",
                quoted_publication_name, transaction_to_track_as_str
            );
            pg_connection
                .execute(&create_all_query, &[])
                .await
                .map_err(to_anyhow)?;
        }
    }

    Ok(())
}

pub async fn alter_publication(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, publication_name, postgres_resource_path)): Path<(String, String, String)>,
    Json(publication_data): Json<PublicationData>,
) -> Result<String> {
    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let tx = pg_connection.transaction().await.map_err(to_anyhow)?;

    let publication = get_publication_scope_and_transaction(tx.client(), &publication_name)
        .await
        .map_err(to_anyhow)?;

    update_pg_publication(
        tx.client(),
        &publication_name,
        publication_data,
        publication.map(|publication| publication.0),
    )
    .await
    .map_err(to_anyhow)?;

    tx.commit().await.map_err(to_anyhow)?;

    Ok(format!(
        "Publication {} updated with success",
        publication_name
    ))
}

pub async fn get_publication_scope_and_transaction(
    pg_connection: &Client,
    publication_name: &str,
) -> Result<Option<(bool, Vec<String>)>> {
    let row_opt = pg_connection
        .query_opt(
            r#"
            SELECT
                puballtables AS all_table,
                pubinsert AS insert,
                pubupdate AS update,
                pubdelete AS delete
            FROM
                pg_publication
            WHERE
                pubname = $1
            "#,
            &[&publication_name],
        )
        .await
        .map_err(to_anyhow)?;

    let row = match row_opt {
        Some(r) => r,
        None => return Ok(None),
    };

    let all_table: bool = row.get("all_table");
    let pub_insert: bool = row.get("insert");
    let pub_update: bool = row.get("update");
    let pub_delete: bool = row.get("delete");

    let mut transaction_to_track = Vec::with_capacity(3);
    if pub_insert {
        transaction_to_track.push("insert".to_string());
    }
    if pub_update {
        transaction_to_track.push("update".to_string());
    }
    if pub_delete {
        transaction_to_track.push("delete".to_string());
    }

    Ok(Some((all_table, transaction_to_track)))
}

pub async fn get_tracked_relations(
    pg_connection: &Client,
    publication_name: &str,
) -> Result<Vec<Relations>> {
    let pg_version = get_postgres_version_internal(pg_connection).await?;

    let query = if pg_version.starts_with("14") {
        r#"
        SELECT
            schemaname AS schema_name,
            tablename AS table_name,
            NULL::text[] AS columns,
            NULL::text AS where_clause
        FROM
            pg_publication_tables
        WHERE
            pubname = $1;
        "#
    } else {
        r#"
        SELECT
            schemaname AS schema_name,
            tablename AS table_name,
            attnames AS columns,
            rowfilter AS where_clause
        FROM
            pg_publication_tables
        WHERE
            pubname = $1;
        "#
    };

    let rows = pg_connection
        .query(query, &[&publication_name])
        .await
        .map_err(to_anyhow)?;

    let mut table_to_track: HashMap<String, Relations> = HashMap::new();

    for row in rows {
        let schema_name: Option<String> = row.get("schema_name");
        let table_name: Option<String> = row.get("table_name");
        let columns: Option<Vec<String>> = row.get("columns");
        let where_clause: Option<String> = row.get("where_clause");

        let schema_name = schema_name.ok_or_else::<Error, _>( || {
            anyhow!(
                "Unexpected NULL `schema_name` in publication entry (pubname: `{}`). This should never happen unless PostgreSQL internals are corrupted.",
                publication_name,
            ).into()
        }
        )?;

        let table_name = table_name.ok_or_else::<Error, _>(|| {
            anyhow!(
                "Unexpected NULL `table_name` for schema `{}` in publication `{}`. This should never happen unless PostgreSQL internals are corrupted.",
                schema_name,
                publication_name,
            ).into()
        })?;

        let entry = table_to_track.entry(schema_name.clone());
        let table_to_track = TableToTrack::new(table_name, where_clause, columns);

        match entry {
            std::collections::hash_map::Entry::Occupied(mut occuped) => {
                occuped.get_mut().add_new_table(table_to_track);
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(Relations::new(schema_name, vec![table_to_track]));
            }
        }
    }

    Ok(table_to_track.into_values().collect_vec())
}

pub async fn get_postgres_trigger(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<PostgresTrigger> {
    let path = path.to_path();
    check_scopes(&authed, || format!("postgres_triggers:read:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;
    let trigger = sqlx::query_as!(
        PostgresTrigger,
        r#"
        SELECT
            workspace_id,
            path,
            script_path,
            is_flow,
            edited_by,
            email,
            edited_at,
            server_id,
            last_server_ping,
            extra_perms,
            error,
            enabled,
            replication_slot_name,
            publication_name,
            postgres_resource_path,
            error_handler_path,
            error_handler_args as "error_handler_args: _",
            retry as "retry: _"
        FROM 
            postgres_trigger
        WHERE 
            workspace_id = $1 AND 
            path = $2
        "#,
        &w_id,
        &path
    )
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let trigger = not_found_if_none(trigger, "Trigger", path)?;

    Ok(Json(trigger))
}

pub async fn update_postgres_trigger(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
    Json(postgres_trigger): Json<EditPostgresTrigger>,
) -> Result<String> {
    let workspace_path = path.to_path();
    check_scopes(&authed, || format!("postgres_triggers:write:{}", workspace_path))?;

    let EditPostgresTrigger {
        replication_slot_name,
        publication_name,
        script_path,
        path,
        is_flow,
        postgres_resource_path,
        publication,
        error_handler_path,
        error_handler_args,
        retry,
    } = postgres_trigger;

    let mut pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let exists =
        check_if_logical_replication_slot_exist(&mut pg_connection, &replication_slot_name).await?;

    let tx = pg_connection.transaction().await.map_err(to_anyhow)?;

    if !exists {
        tracing::debug!(
            "Logical replication slot named: {} does not exists creating it...",
            &replication_slot_name
        );
        create_logical_replication_slot(tx.client(), &replication_slot_name)
            .await
            .map_err(to_anyhow)?;
    }

    if let Some(publication) = publication {
        let publication_data =
            get_publication_scope_and_transaction(tx.client(), &publication_name)
                .await
                .map_err(to_anyhow)?;

        update_pg_publication(
            tx.client(),
            &publication_name,
            publication,
            publication_data.map(|publication| publication.0),
        )
        .await
        .map_err(to_anyhow)?;
    }

    tx.commit().await.map_err(to_anyhow)?;

    let mut tx = user_db.begin(&authed).await?;

    sqlx::query!(
        r#"
            UPDATE postgres_trigger 
            SET 
                script_path = $1, 
                path = $2, 
                is_flow = $3, 
                edited_by = $4, 
                email = $5, 
                postgres_resource_path = $6, 
                replication_slot_name = $7,
                publication_name = $8,
                edited_at = now(), 
                error = NULL,
                server_id = NULL,
                error_handler_path = $11,
                error_handler_args = $12,
                retry = $13
            WHERE 
                workspace_id = $9 AND 
                path = $10
            "#,
        script_path,
        path,
        is_flow,
        &authed.username,
        &authed.email,
        postgres_resource_path,
        replication_slot_name,
        publication_name,
        w_id,
        workspace_path,
        error_handler_path,
        error_handler_args as _,
        retry as _,
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "postgres_triggers.update",
        ActionKind::Update,
        &w_id,
        Some(&path),
        None,
    )
    .await?;

    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::PostgresTrigger { path: path.to_string() },
        Some(format!("Postgres trigger '{}' updated", path)),
        true,
    )
    .await?;

    Ok(workspace_path.to_string())
}

pub async fn delete_postgres_trigger(
    authed: ApiAuthed,
    Extension(db): Extension<DB>,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> Result<String> {
    let path = path.to_path();
    check_scopes(&authed, || format!("postgres_triggers:write:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;
    sqlx::query!(
        r#"
        DELETE FROM postgres_trigger 
        WHERE 
            workspace_id = $1 AND 
            path = $2
        "#,
        w_id,
        path,
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "postgres_triggers.delete",
        ActionKind::Delete,
        &w_id,
        Some(path),
        None,
    )
    .await?;

    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::PostgresTrigger { path: path.to_string() },
        Some(format!("Postgres trigger '{}' deleted", path)),
        true,
    )
    .await?;

    Ok(format!("Postgres trigger {path} deleted"))
}

pub async fn exists_postgres_trigger(
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<bool> {
    let path = path.to_path();
    let exists = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 
            FROM postgres_trigger 
            WHERE 
                path = $1 AND 
                workspace_id = $2
        )"#,
        path,
        w_id,
    )
    .fetch_one(&db)
    .await?
    .unwrap_or(false);
    Ok(Json(exists))
}

pub async fn set_enabled(
    authed: ApiAuthed,
    Extension(db): Extension<DB>,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
    Json(payload): Json<SetEnabled>,
) -> Result<String> {
    let path = path.to_path();
    check_scopes(&authed, || format!("postgres_triggers:write:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;

    // important to set server_id, last_server_ping and error to NULL to stop current postgres listener
    let one_o = sqlx::query_scalar!(
        r#"
        UPDATE postgres_trigger 
        SET 
            enabled = $1, 
            email = $2, 
            edited_by = $3, 
            edited_at = now(), 
            server_id = NULL, 
            error = NULL
        WHERE 
            path = $4 AND 
            workspace_id = $5 
        RETURNING 1
        "#,
        payload.enabled,
        &authed.email,
        &authed.username,
        path,
        w_id,
    )
    .fetch_optional(&mut *tx)
    .await?
    .flatten();

    not_found_if_none(one_o, "Postgres trigger", path)?;

    audit_log(
        &mut *tx,
        &authed,
        "postgres_triggers.setenabled",
        ActionKind::Update,
        &w_id,
        Some(path),
        Some([("enabled", payload.enabled.to_string().as_ref())].into()),
    )
    .await?;

    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::PostgresTrigger { path: path.to_string() },
        Some(format!("Postgres trigger '{}' updated", path)),
        true,
    )
    .await?;

    Ok(format!(
        "succesfully updated postgres trigger at path {} to status {}",
        path, payload.enabled
    ))
}

pub async fn get_template_script(Path((_, id)): Path<(String, String)>) -> Result<String> {
    let template = if let Some((_, template)) = TEMPLATE.remove(&id) {
        template
    } else {
        "".to_string()
    };
    Ok(template)
}

pub async fn create_template_script(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path(w_id): Path<String>,
    Json(template_script): Json<TemplateScript>,
) -> Result<String> {
    let TemplateScript { postgres_resource_path, relations, language } = template_script;

    let relations = match relations {
        Some(r) => r,
        None => return Err(anyhow!("You must at least choose schema to fetch table from").into()),
    };

    let pg_connection: Client = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let mut schema_or_fully_qualified_name = Vec::with_capacity(relations.len());
    let mut columns_list = Vec::with_capacity(relations.len());

    for relation in relations {
        if !relation.table_to_track.is_empty() {
            for table in relation.table_to_track {
                let fully_qualified_name = format!("{}.{}", relation.schema_name, table.table_name);
                schema_or_fully_qualified_name.push(quote_literal(&fully_qualified_name));
                let columns = table
                    .columns_name
                    .map(|c| quote_literal(&c.join(",")))
                    .unwrap_or_else(|| "''".to_string());
                columns_list.push(columns);
            }
        } else {
            schema_or_fully_qualified_name.push(quote_literal(&relation.schema_name));
            columns_list.push("''".to_string());
        }
    }

    let tables_name = schema_or_fully_qualified_name.join(",");
    let columns_list = columns_list.join(",");

    let query = format!(
        r#"
        WITH table_column_mapping AS (
            SELECT
                unnest(ARRAY[{}]) AS table_name,
                unnest(ARRAY[{}]) AS column_list
        ),
        parsed_columns AS (
            SELECT
                tcm.table_name,
                CASE
                    WHEN tcm.column_list = '' THEN NULL
                    ELSE string_to_array(tcm.column_list, ',')
                END AS columns
            FROM table_column_mapping tcm
        )
        SELECT
            ns.nspname AS table_schema,
            cls.relname AS table_name,
            attr.attname AS column_name,
            attr.atttypid AS oid,
            attr.attnotnull AS is_nullable
        FROM pg_attribute attr
        JOIN pg_class cls ON attr.attrelid = cls.oid
        JOIN pg_namespace ns ON cls.relnamespace = ns.oid
        JOIN parsed_columns pc
            ON ns.nspname || '.' || cls.relname = pc.table_name
            OR ns.nspname = pc.table_name
        WHERE
            attr.attnum > 0
            AND NOT attr.attisdropped
            AND cls.relkind = 'r'
            AND (
                pc.columns IS NULL
                OR attr.attname = ANY(pc.columns)
            );
        "#,
        tables_name, columns_list
    );

    let rows = pg_connection.query(&query, &[]).await.map_err(to_anyhow)?;

    let mut schema_map: HashMap<String, HashMap<String, Vec<MappingInfo>>> = HashMap::new();

    #[derive(Debug)]
    struct ColumnInfo {
        table_schema: String,
        table_name: String,
        column_name: String,
        oid: u32,
        is_nullable: bool,
    }

    for row in rows {
        let info = ColumnInfo {
            table_schema: row.get("table_schema"),
            table_name: row.get("table_name"),
            column_name: row.get("column_name"),
            oid: row.get::<_, u32>("oid"),
            is_nullable: row.get::<_, bool>("is_nullable"),
        };

        let mapped_info =
            MappingInfo::new(info.column_name, Type::from_oid(info.oid), info.is_nullable);

        match schema_map.entry(info.table_schema) {
            Occupied(mut schema_entry) => match schema_entry.get_mut().entry(info.table_name) {
                Occupied(mut table_entry) => {
                    table_entry.get_mut().push(mapped_info);
                }
                Vacant(v) => {
                    v.insert(vec![mapped_info]);
                }
            },
            Vacant(schema_vacant) => {
                let mut table_map = HashMap::new();
                table_map.insert(info.table_name, vec![mapped_info]);
                schema_vacant.insert(table_map);
            }
        }
    }

    let mapper = Mapper::new(schema_map, language);
    let template = mapper.get_template();

    let id = format!("{}-{}", w_id, uuid::Uuid::new_v4());

    TEMPLATE.insert(id.clone(), template);

    Ok(id)
}

pub async fn is_database_in_logical_level(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, postgres_resource_path)): Path<(String, String)>,
) -> error::JsonResult<bool> {
    let pg_connection = get_default_pg_connection(
        authed.clone(),
        Some(user_db.clone()),
        &db,
        &postgres_resource_path,
        &w_id,
    )
    .await
    .map_err(to_anyhow)?;

    let row_opt = pg_connection
        .query_opt("SHOW wal_level;", &[])
        .await
        .map_err(to_anyhow)?;

    let wal_level: Option<String> = row_opt.map(|row| row.get(0));

    let is_logical = matches!(wal_level.as_deref(), Some("logical"));

    Ok(Json(is_logical))
}

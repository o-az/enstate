use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Redirect;

use crate::routes::{
    http_simple_status_error, profile_http_error_mapper, universal_profile_resolve, FreshQuery,
    RouteError,
};

// #[utoipa::path(
//     get,
//     path = "/h/{name_or_address}",
//     responses(
//         TODO: figure out body
//         (status = 200, description = "Successfully found name or address.", body = ENSProfile),
//         (status = NOT_FOUND, description = "No name or address could be found."),
//     ),
//     params(
//         ("name_or_address" = String, Path, description = "Name or address to lookup the header for."),
//     )
// )]
pub async fn get(
    Path(name_or_address): Path<String>,
    Query(query): Query<FreshQuery>,
    State(state): State<Arc<crate::AppState>>,
) -> Result<Redirect, RouteError> {
    let rpc = state
        .provider
        .get_provider()
        .ok_or_else(|| http_simple_status_error(StatusCode::INTERNAL_SERVER_ERROR))?
        .clone();

    let profile =
        universal_profile_resolve(&name_or_address, query.fresh.unwrap_or(false), rpc, &state)
            .await
            .map_err(profile_http_error_mapper)?;

    if let Some(header) = profile.header {
        return Ok(Redirect::to(header.as_str()));
    }

    Err(http_simple_status_error(StatusCode::NOT_FOUND))
}
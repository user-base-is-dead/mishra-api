//! Admin UI routeconfig

use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode, Uri, header},
    response::IntoResponse,
    routing::get,
};
use rust_embed::Embed;

/// embed the frontend build artifacts
#[derive(Embed)]
#[folder = "admin-ui/dist"]
struct Asset;

/// create Admin UI route
pub fn create_admin_ui_router() -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/{*file}", get(static_handler))
}

/// handlefirstpagerequest
async fn index_handler() -> impl IntoResponse {
    serve_index()
}

/// handle static file requests
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // security check: reject those containing .. path
    if path.contains("..") {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid path"))
            .expect("Failed to build response");
    }

    // attempt to get the requested file
    if let Some(content) = Asset::get(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        // Sets different cache strategies based on the file type.
        let cache_control = get_cache_control(path);

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, cache_control)
            .body(Body::from(content.data.into_owned()))
            .expect("Failed to build response");
    }

    // SPA fallback: If the file does not exist and is not a resource file, returns index.html
    if !is_asset_path(path) {
        return serve_index();
    }

    // 404
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not found"))
        .expect("Failed to build response")
}

/// provide index.html
fn serve_index() -> Response<Body> {
    match Asset::get("index.html") {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(content.data.into_owned()))
            .expect("Failed to build response"),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(
                "Admin UI not built. Run 'bun run build' in admin-ui directory.",
            ))
            .expect("Failed to build response"),
    }
}

/// Returns an appropriate cache strategy based on the file type.
fn get_cache_control(path: &str) -> &'static str {
    if path.ends_with(".html") {
        // HTML The file is not cached, ensuring the user gets the latest version.
        "no-cache"
    } else if path.starts_with("assets/") {
        // assets/ Files under the directory carry a content hash and can be cached long term.
        "public, max-age=31536000, immutable"
    } else {
        // otherfile(such as favicon) use a shorter cache
        "public, max-age=3600"
    }
}

/// Determines whether it is a resource file path (a file with an extension).
fn is_asset_path(path: &str) -> bool {
    // Checks whether the last path segment contains an extension.
    path.rsplit('/')
        .next()
        .map(|filename| filename.contains('.'))
        .unwrap_or(false)
}

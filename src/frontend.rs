use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::Response,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct FrontendAssets;

pub async fn serve(uri: Uri) -> Response {
    let requested_path = uri.path().trim_start_matches('/');
    let asset_path = if requested_path.is_empty() {
        "index.html"
    } else {
        requested_path
    };

    let embedded_asset = FrontendAssets::get(asset_path)
        .map(|asset| (asset_path, asset))
        .or_else(|| FrontendAssets::get("index.html").map(|asset| ("index.html", asset)));

    match embedded_asset {
        Some((served_path, asset)) => {
            let content_type = mime_guess::from_path(served_path)
                .first_or_octet_stream()
                .as_ref()
                .to_owned();

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(asset.data))
                .expect("embedded asset response must be valid")
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("frontend asset not found"))
            .expect("not-found response must be valid"),
    }
}

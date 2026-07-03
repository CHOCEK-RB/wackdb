//! Web Visualizer Server for `WackDB`.
//! This crate provides an HTTP API to inspect the internal state of the Buffer Pool, Heap pages, and B-Tree nodes.

#![allow(unused_crate_dependencies, clippy::cast_ptr_alignment)]

pub mod dtos;
pub mod handlers;
pub mod state;

use axum::{Router, routing::get};
use tower_http::cors::{Any, CorsLayer};

use handlers::{
    fetch_and_get_btree_page, fetch_and_get_page, get_buffer_pool_state, get_frame_content,
    get_table_index_pages, get_table_pages, get_tables,
};
use state::{AppState, SharedBufferPool};

/// Starts the web server in the background.
///
/// # Errors
///
/// Returns an error if the server fails to bind to the specified port.
pub async fn start_server(
    buffer_pool: SharedBufferPool,
    data_dir: String,
    port: u16,
) -> Result<(), std::io::Error> {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);

    let state = AppState {
        buffer_pool,
        data_dir,
    };

    let app = Router::new()
        .route("/api/buffer_pool", get(get_buffer_pool_state))
        .route("/api/frame/:frame_id", get(get_frame_content))
        .route("/api/catalog/tables", get(get_tables))
        .route("/api/catalog/table/:name/pages", get(get_table_pages))
        .route(
            "/api/catalog/table/:name/index_pages",
            get(get_table_index_pages),
        )
        .route("/api/page/:file_id/:page_num", get(fetch_and_get_page))
        .route(
            "/api/btree_page/:file_id/:page_num",
            get(fetch_and_get_btree_page),
        )
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Web visualizer server running on http://{addr}");

    axum::serve(listener, app).await
}

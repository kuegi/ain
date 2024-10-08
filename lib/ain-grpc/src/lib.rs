#[macro_use]
extern crate serde;
extern crate serde_json;

pub mod block;
pub mod call_request;
pub mod codegen;
mod errors;
mod filters;
mod impls;
pub mod logging;
mod logs;
mod receipt;
pub mod rpc;
mod subscription;
mod sync;
mod trace;
mod transaction;
mod transaction_request;
mod utils;
use defichain_rpc::{Auth, Client};

#[cfg(test)]
mod tests;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};

use ain_evm::services::{IS_SERVICES_INIT_CALL, SERVICES};
use ain_ocean::SERVICES as OCEAN_SERVICES;
use anyhow::{format_err, Result};
use hyper::{header::HeaderValue, Method};
use jsonrpsee::core::server::rpc_module::Methods;
use jsonrpsee_server::ServerBuilder;
use log::info;
use logging::CppLogTarget;
use tower_http::cors::CorsLayer;

use crate::{
    rpc::{
        debug::{MetachainDebugRPCModule, MetachainDebugRPCServer},
        eth::{MetachainRPCModule, MetachainRPCServer},
        net::{MetachainNetRPCModule, MetachainNetRPCServer},
        web3::{MetachainWeb3RPCModule, MetachainWeb3RPCServer},
    },
    subscription::{
        eth::{MetachainPubSubModule, MetachainPubSubServer},
        MetachainSubIdProvider,
    },
};

// TODO: Ideally most of the below and SERVICES needs to go into its own core crate now,
// and this crate be dedicated to network services.
// Note: This cannot just move to rs-exports, since rs-exports cannot have reverse
// deps that depend on it.

pub fn preinit() {}

pub fn init_logging() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(log::Level::Info.as_str()),
    )
    .format(logging::cpp_log_target_format)
    .target(env_logger::Target::Pipe(Box::new(CppLogTarget::new(false))))
    .init();
    info!("Init rs logging");
}

pub fn init_services() {
    info!("Init rs services");
    let _ = &*SERVICES;
}

pub fn init_network_json_rpc_service(addr: String) -> Result<()> {
    info!("Init rs rpc services");

    let addr = addr.as_str().parse::<SocketAddr>()?;
    let max_connections = ain_cpp_imports::get_max_connections();
    let max_response_size = ain_cpp_imports::get_max_response_byte_size();
    let services = &SERVICES;

    let middleware = if !ain_cpp_imports::get_cors_allowed_origin().is_empty() {
        let origin = ain_cpp_imports::get_cors_allowed_origin();
        info!("Allowed origins: {}", origin);
        let cors = CorsLayer::new()
            .allow_methods([Method::POST, Method::GET, Method::OPTIONS])
            .allow_origin(origin.parse::<HeaderValue>()?)
            .allow_headers([hyper::header::CONTENT_TYPE, hyper::header::AUTHORIZATION])
            .allow_credentials(origin != "*");

        tower::ServiceBuilder::new().layer(cors)
    } else {
        tower::ServiceBuilder::new().layer(CorsLayer::new())
    };

    let server = services.runtime().block_on(
        ServerBuilder::default()
            .set_middleware(middleware)
            .max_connections(max_connections)
            .max_response_body_size(max_response_size)
            .custom_tokio_runtime(services.runtime().handle().clone())
            .build(addr),
    )?;

    let local_addr = server.local_addr()?;
    info!("Starting JSON Eth RPC server at {}", local_addr);
    ain_cpp_imports::print_port_usage(2, local_addr.port());

    let mut methods: Methods = Methods::new();
    methods.merge(MetachainRPCModule::new(Arc::clone(&services.evm)).into_rpc())?;
    methods.merge(MetachainDebugRPCModule::new(Arc::clone(&services.evm)).into_rpc())?;
    methods.merge(MetachainNetRPCModule::new(Arc::clone(&services.evm)).into_rpc())?;
    methods.merge(MetachainWeb3RPCModule::new(Arc::clone(&services.evm)).into_rpc())?;

    services
        .json_rpc_handles
        .lock()
        .push(server.start(methods)?);
    Ok(())
}

pub async fn init_ocean_server(addr: String) -> Result<()> {
    let addr = addr.parse::<SocketAddr>()?;
    let services = &SERVICES;

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let local_addr = listener.local_addr()?;
    info!("Starting ocean server at {}", local_addr);
    ain_cpp_imports::print_port_usage(4, local_addr.port());

    let (user, pass) = ain_cpp_imports::get_rpc_auth().map_err(|e| format_err!("{e}"))?;
    let client = Arc::new(
        Client::new(
            &format!("http://127.0.0.1:{}", ain_cpp_imports::get_rpc_port()),
            Auth::UserPass(user, pass),
        )
        .await?,
    );
    let network = ain_cpp_imports::get_network();

    let ocean_router = ain_ocean::ocean_router(&OCEAN_SERVICES, client, network).await?;

    let server_handle = services.runtime().spawn(async move {
        if let Err(e) = axum::serve(listener, ocean_router).await {
            log::error!("Server encountered an error: {}", e);
        }
    });

    *services.ocean_handle.lock() = Some(server_handle);
    Ok(())
}

pub fn init_network_rest_ocean(addr: String) -> Result<()> {
    info!("Starting REST Ocean server at {}", addr);
    SERVICES.runtime().block_on(init_ocean_server(addr))
}

pub fn init_network_grpc_service(_addr: String) -> Result<()> {
    // log::info!("Starting gRPC server at {}", addr);
    // Commented out for now as nothing to serve
    // let runtime = &SERVICES;
    // runtime
    //     .rt_handle
    // .spawn(Server::builder().serve(addr.parse()?));
    Ok(())
}

pub fn init_network_subscriptions_service(addr: String) -> Result<()> {
    info!("Init rs ws services");

    let addr = addr.as_str().parse::<SocketAddr>()?;
    let max_connections = ain_cpp_imports::get_max_connections();
    let max_response_size = ain_cpp_imports::get_max_response_byte_size();
    let services = &SERVICES;

    let server = services.runtime().block_on(
        ServerBuilder::default()
            .max_subscriptions_per_connection(max_connections)
            .max_response_body_size(max_response_size)
            .custom_tokio_runtime(services.runtime().handle().clone())
            .set_id_provider(MetachainSubIdProvider)
            .build(addr),
    )?;

    let local_addr = server.local_addr()?;
    info!("Starting ws server at {}", local_addr);
    ain_cpp_imports::print_port_usage(3, local_addr.port());

    let mut methods: Methods = Methods::new();
    methods.merge(
        MetachainPubSubModule::new(
            Arc::clone(&services.evm),
            services.runtime().handle().clone(),
        )
        .into_rpc(),
    )?;

    services
        .websocket_handles
        .lock()
        .push(server.start(methods)?);
    Ok(())
}

fn is_services_init_called() -> bool {
    IS_SERVICES_INIT_CALL.load(Ordering::SeqCst)
}

pub fn stop_services() -> Result<()> {
    if is_services_init_called() {
        info!("Shutdown rs services");
        SERVICES.stop()?;
    }
    Ok(())
}

pub fn wipe_evm_folder() -> Result<()> {
    let datadir = ain_cpp_imports::get_datadir();
    let path = PathBuf::from(datadir).join("evm");
    info!("Wiping rs storage in {}", path.display());
    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|e| format_err!("Error wiping evm dir: {e}"))?;
    }
    Ok(())
}

pub fn stop_network_services() -> Result<()> {
    if is_services_init_called() {
        info!("Shutdown rs network services");
        SERVICES.stop_network()?;
    }
    Ok(())
}

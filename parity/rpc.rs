// Copyright 2015-2017 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

use std::fmt;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io;

use dir::default_data_path;
use ethcore_rpc::{self as rpc, HttpServerError, Metadata, Origin, AccessControlAllowOrigin, Host};
use ethcore_rpc::informant::{RpcStats, Middleware};
use helpers::parity_ipc_path;
use hyper;
use jsonrpc_core::MetaIoHandler;
use rpc_apis;
use rpc_apis::ApiSet;
use parity_reactor::TokioRemote;

pub use ethcore_rpc::{IpcServer, HttpServer};

#[derive(Debug, PartialEq)]
pub struct HttpConfiguration {
	pub enabled: bool,
	pub interface: String,
	pub port: u16,
	pub apis: ApiSet,
	pub cors: Option<Vec<String>>,
	pub hosts: Option<Vec<String>>,
}

impl Default for HttpConfiguration {
	fn default() -> Self {
		HttpConfiguration {
			enabled: true,
			interface: "127.0.0.1".into(),
			port: 8545,
			apis: ApiSet::UnsafeContext,
			cors: None,
			hosts: Some(Vec::new()),
		}
	}
}

#[derive(Debug, PartialEq)]
pub struct IpcConfiguration {
	pub enabled: bool,
	pub socket_addr: String,
	pub apis: ApiSet,
}

impl Default for IpcConfiguration {
	fn default() -> Self {
		let data_dir = default_data_path();
		IpcConfiguration {
			enabled: true,
			socket_addr: parity_ipc_path(&data_dir, "$BASE/jsonrpc.ipc"),
			apis: ApiSet::IpcContext,
		}
	}
}

impl fmt::Display for IpcConfiguration {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		if self.enabled {
			write!(f, "endpoint address [{}], api list [{:?}]", self.socket_addr, self.apis)
		} else {
			write!(f, "disabled")
		}
	}
}

pub struct Dependencies {
	pub apis: Arc<rpc_apis::Dependencies>,
	pub remote: TokioRemote,
	pub stats: Arc<RpcStats>,
}

pub struct RpcExtractor;
impl rpc::HttpMetaExtractor<Metadata> for RpcExtractor {
	fn read_metadata(&self, req: &hyper::server::Request<hyper::net::HttpStream>) -> Metadata {
		let origin = req.headers().get::<hyper::header::Origin>()
			.map(|origin| format!("{}://{}", origin.scheme, origin.host))
			.unwrap_or_else(|| "unknown".into());
		let mut metadata = Metadata::default();
		metadata.origin = Origin::Rpc(origin);
		metadata
	}
}

impl rpc::IpcMetaExtractor<Metadata> for RpcExtractor {
	fn extract(&self, _req: &rpc::IpcRequestContext) -> Metadata {
		let mut metadata = Metadata::default();
		// TODO [ToDr] Extract proper session id when it's available in context.
		metadata.origin = Origin::Ipc(1.into());
		metadata
	}
}

pub fn new_http(conf: HttpConfiguration, deps: &Dependencies) -> Result<Option<HttpServer>, String> {
	if !conf.enabled {
		return Ok(None);
	}

	let url = format!("{}:{}", conf.interface, conf.port);
	let addr = url.parse().map_err(|_| format!("Invalid JSONRPC listen host/port given: {}", url))?;
	Ok(Some(setup_http_rpc_server(deps, &addr, conf.cors, conf.hosts, conf.apis)?))
}

fn setup_apis(apis: ApiSet, deps: &Dependencies) -> MetaIoHandler<Metadata, Middleware> {
	rpc_apis::setup_rpc(deps.stats.clone(), deps.apis.clone(), apis)
}

pub fn setup_http_rpc_server(
	dependencies: &Dependencies,
	url: &SocketAddr,
	cors_domains: Option<Vec<String>>,
	allowed_hosts: Option<Vec<String>>,
	apis: ApiSet
) -> Result<HttpServer, String> {
	let handler = setup_apis(apis, dependencies);
	let remote = dependencies.remote.clone();
	let cors_domains: Option<Vec<_>> = cors_domains.map(|domains| domains.into_iter().map(AccessControlAllowOrigin::from).collect());
	let allowed_hosts: Option<Vec<_>> = allowed_hosts.map(|hosts| hosts.into_iter().map(Host::from).collect());
	let start_result = rpc::start_http(url, cors_domains.into(), allowed_hosts.into(), handler, remote, RpcExtractor);
	match start_result {
		Err(HttpServerError::IoError(err)) => match err.kind() {
			io::ErrorKind::AddrInUse => Err(format!("RPC address {} is already in use, make sure that another instance of an Ethereum client is not running or change the address using the --jsonrpc-port and --jsonrpc-interface options.", url)),
			_ => Err(format!("RPC io error: {}", err)),
		},
		Err(e) => Err(format!("RPC error: {:?}", e)),
		Ok(server) => Ok(server),
	}
}

pub fn new_ipc(conf: IpcConfiguration, deps: &Dependencies) -> Result<Option<IpcServer>, String> {
	if !conf.enabled { return Ok(None); }
	Ok(Some(setup_ipc_rpc_server(deps, &conf.socket_addr, conf.apis)?))
}

pub fn setup_ipc_rpc_server(dependencies: &Dependencies, addr: &str, apis: ApiSet) -> Result<IpcServer, String> {
	let handler = setup_apis(apis, dependencies);
	let remote = dependencies.remote.clone();
	match rpc::start_ipc(addr, handler, remote, RpcExtractor) {
		Err(io_error) => Err(format!("RPC io error: {}", io_error)),
		Ok(server) => Ok(server)
	}
}
